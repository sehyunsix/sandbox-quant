use super::*;

pub(super) fn switch_timeframe(
    instrument: &str,
    interval: &str,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    let interval = interval.to_string();
    let rest = rest_client.clone();
    let instrument = instrument.to_string();
    let limit = config.ui.price_history_len;
    let tx = app_tx.clone();
    let interval_ms = match parse_interval_ms(&interval) {
        Ok(ms) => ms,
        Err(e) => {
            let err_tx = tx.clone();
            tokio::spawn(async move {
                let _ = err_tx
                    .send(AppEvent::Error(format!(
                        "Invalid timeframe '{}': {}",
                        interval, e
                    )))
                    .await;
            });
            return;
        }
    };
    let iv = interval.clone();
    tokio::spawn(async move {
        let (symbol, market) = parse_instrument_label(&instrument);
        match rest
            .get_klines_for_market(&symbol, &iv, limit, market == MarketKind::Futures)
            .await
        {
            Ok(candles) => {
                let _ = tx
                    .send(AppEvent::HistoricalCandles {
                        candles,
                        interval_ms,
                        interval,
                    })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(AppEvent::Error(format!("Kline fetch failed: {}", e)))
                    .await;
            }
        }
    });
}

pub(super) fn parse_instrument_label(label: &str) -> (String, MarketKind) {
    let trimmed = label.trim();
    if let Some(sym) = trimmed.strip_suffix(" (FUT)") {
        return (sym.to_ascii_uppercase(), MarketKind::Futures);
    }
    (trimmed.to_ascii_uppercase(), MarketKind::Spot)
}

pub(super) fn normalize_instrument_label(label: &str) -> String {
    let (symbol, market) = parse_instrument_label(label);
    if market == MarketKind::Futures {
        format!("{} (FUT)", symbol)
    } else {
        symbol
    }
}

pub(super) fn close_all_reason_code(job_id: u64) -> String {
    format!("ui.close_all#{}", job_id)
}

pub(super) fn parse_close_all_job_id(reason_code: &str) -> Option<u64> {
    reason_code
        .strip_prefix("ui.close_all#")
        .and_then(|v| v.parse::<u64>().ok())
}

pub(super) fn close_all_soft_skip_reason(reason_code: &str) -> bool {
    matches!(
        reason_code,
        "risk.qty_too_small" | "risk.no_spot_base_balance" | "risk.insufficient_base_balance"
    )
}

pub(super) fn build_asset_pnl_snapshot(
    order_managers: &HashMap<String, OrderManager>,
    realized_pnl_by_symbol: &HashMap<String, f64>,
) -> HashMap<String, AssetPnlEntry> {
    order_managers
        .iter()
        .map(|(symbol, mgr)| {
            (
                symbol.clone(),
                AssetPnlEntry {
                    is_futures: mgr.market_kind() == MarketKind::Futures,
                    side: mgr.position().side,
                    position_qty: mgr.position().qty,
                    entry_price: mgr.position().entry_price,
                    realized_pnl_usdt: realized_pnl_by_symbol.get(symbol).copied().unwrap_or(0.0),
                    unrealized_pnl_usdt: mgr.position().unrealized_pnl,
                },
            )
        })
        .collect()
}

pub(super) fn canonical_strategy_tag(tag: &str) -> String {
    match tag.trim() {
        "MA(Config)" => "cfg".to_string(),
        "MA(Fast 5/20)" => "fst".to_string(),
        "MA(Slow 20/60)" => "slw".to_string(),
        "MANUAL" => "mnl".to_string(),
        other => other.to_ascii_lowercase(),
    }
}

pub(super) fn strategy_stats_scope_key(instrument: &str, tag: &str) -> String {
    format!(
        "{}::{}",
        normalize_instrument_label(instrument),
        canonical_strategy_tag(tag)
    )
}

pub(super) fn build_scoped_strategy_stats(
    by_instrument: &HashMap<String, HashMap<String, OrderHistoryStats>>,
) -> HashMap<String, OrderHistoryStats> {
    let mut aggregated: HashMap<String, OrderHistoryStats> = HashMap::new();
    for (instrument, stats_by_tag) in by_instrument {
        for (tag, s) in stats_by_tag {
            let scoped_key = strategy_stats_scope_key(instrument, tag);
            let slot = aggregated.entry(scoped_key).or_default();
            slot.trade_count = slot.trade_count.saturating_add(s.trade_count);
            slot.win_count = slot.win_count.saturating_add(s.win_count);
            slot.lose_count = slot.lose_count.saturating_add(s.lose_count);
            slot.realized_pnl += s.realized_pnl;
        }
    }
    aggregated
}

pub(super) fn app_log(
    level: LogLevel,
    domain: LogDomain,
    event: &'static str,
    msg: impl Into<String>,
) -> AppEvent {
    AppEvent::LogRecord(LogRecord::new(level, domain, event, msg))
}

pub(super) fn derived_stop_price(position: &Position, stop_loss_pct: f64) -> Option<f64> {
    if position.qty.abs() <= f64::EPSILON || position.entry_price <= f64::EPSILON {
        return None;
    }
    let pct = stop_loss_pct.max(0.0);
    match position.side {
        Some(sandbox_quant::model::order::OrderSide::Sell) => {
            Some(position.entry_price * (1.0 + pct))
        }
        _ => Some(position.entry_price * (1.0 - pct)),
    }
}

pub(super) fn y_snapshot_for_entry(
    ev_cfg: &EvEstimatorConfig,
    market: MarketKind,
    futures_multiplier: f64,
    y: YNormal,
    order_amount_usdt: f64,
    entry_price: f64,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    if entry_price <= f64::EPSILON || order_amount_usdt <= f64::EPSILON {
        return None;
    }
    let qty = order_amount_usdt / entry_price;
    if qty <= f64::EPSILON {
        return None;
    }
    let stats = if market == MarketKind::Futures {
        futures_ev_from_y_normal(
            y,
            FuturesEvInputs {
                p0: entry_price,
                qty,
                multiplier: futures_multiplier,
                side: PositionSide::Long,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                funding: 0.0,
                liq_risk: 0.0,
            },
        )
    } else {
        spot_ev_from_y_normal(
            y,
            SpotEvInputs {
                p0: entry_price,
                qty,
                side: PositionSide::Long,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                borrow: 0.0,
            },
        )
    };
    Some(EntryExpectancySnapshot {
        expected_return_usdt: stats.ev,
        expected_holding_ms: max_holding_ms.max(1),
        worst_case_loss_usdt: stats.ev_std,
        fee_slippage_penalty_usdt: ev_cfg.fee_slippage_penalty_usdt,
        probability: sandbox_quant::ev::ProbabilitySnapshot {
            p_win: stats.p_win,
            p_tail_loss: 1.0 - stats.p_win,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: sandbox_quant::ev::ConfidenceLevel::Low,
            prob_model_version: "y-normal-v1".to_string(),
        },
        ev_model_version: "y-normal-spot-fut-v1".to_string(),
        computed_at_ms: now_ms,
    })
}

pub(super) fn y_snapshot_for_open_position(
    ev_cfg: &EvEstimatorConfig,
    market: MarketKind,
    futures_multiplier: f64,
    y: YNormal,
    entry_price: f64,
    qty: f64,
    side: Option<sandbox_quant::model::order::OrderSide>,
    max_holding_ms: u64,
    now_ms: u64,
) -> Option<EntryExpectancySnapshot> {
    if entry_price <= f64::EPSILON || qty.abs() <= f64::EPSILON {
        return None;
    }
    let side = match side {
        Some(sandbox_quant::model::order::OrderSide::Sell) => PositionSide::Short,
        _ => PositionSide::Long,
    };
    let stats = if market == MarketKind::Futures {
        futures_ev_from_y_normal(
            y,
            FuturesEvInputs {
                p0: entry_price,
                qty: qty.abs(),
                multiplier: futures_multiplier,
                side,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                funding: 0.0,
                liq_risk: 0.0,
            },
        )
    } else {
        spot_ev_from_y_normal(
            y,
            SpotEvInputs {
                p0: entry_price,
                qty: qty.abs(),
                side,
                fee: 0.0,
                slippage: ev_cfg.fee_slippage_penalty_usdt,
                borrow: 0.0,
            },
        )
    };
    Some(EntryExpectancySnapshot {
        expected_return_usdt: stats.ev,
        expected_holding_ms: max_holding_ms.max(1),
        worst_case_loss_usdt: stats.ev_std,
        fee_slippage_penalty_usdt: ev_cfg.fee_slippage_penalty_usdt,
        probability: sandbox_quant::ev::ProbabilitySnapshot {
            p_win: stats.p_win,
            p_tail_loss: 1.0 - stats.p_win,
            p_timeout_exit: 0.5,
            n_eff: 0.0,
            confidence: sandbox_quant::ev::ConfidenceLevel::Low,
            prob_model_version: "y-normal-v1".to_string(),
        },
        ev_model_version: "y-normal-spot-fut-v1".to_string(),
        computed_at_ms: now_ms,
    })
}

pub(super) fn enabled_instruments(
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
) -> Vec<String> {
    let mut instruments: Vec<String> = strategy_catalog
        .profiles()
        .iter()
        .filter(|profile| enabled_strategy_tags.contains(&profile.source_tag))
        .map(|profile| normalize_instrument_label(&profile.symbol))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    instruments.sort();
    instruments
}

pub(super) fn strategy_instruments_from_profiles(
    profiles_by_tag: &HashMap<String, StrategyProfile>,
    selected_symbol: &str,
) -> Vec<String> {
    let mut instruments: Vec<String> = profiles_by_tag
        .values()
        .map(|profile| normalize_instrument_label(&profile.symbol))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let selected = normalize_instrument_label(selected_symbol);
    if !instruments.iter().any(|item| item == &selected) {
        instruments.push(selected);
    }
    instruments.sort();
    instruments
}

pub(super) fn persist_strategy_session_state(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    current_strategy_profile: &StrategyProfile,
    enabled_strategy_tags: &HashSet<String>,
) {
    if let Err(e) = strategy_session::persist_strategy_session(
        strategy_catalog,
        &current_strategy_profile.source_tag,
        enabled_strategy_tags,
    ) {
        app_state.push_log(format!("[WARN] Failed to save strategy session: {}", e));
    }
}

pub(super) fn refresh_strategy_lists(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
) {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let lifecycle_rows = strategy_catalog.lifecycle_rows(now_ms);
    app_state.strategy_items = strategy_catalog.labels();
    app_state.strategy_item_symbols = strategy_catalog.symbols();
    app_state.strategy_item_active = strategy_catalog
        .profiles()
        .iter()
        .map(|row| enabled_strategy_tags.contains(&row.source_tag) && !app_state.paused)
        .collect();
    app_state.strategy_item_created_at_ms =
        lifecycle_rows.iter().map(|row| row.created_at_ms).collect();
    app_state.strategy_item_total_running_ms = lifecycle_rows
        .iter()
        .map(|row| row.total_running_ms)
        .collect();
}

pub(super) fn publish_strategy_runtime_updates(
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
    strategy_profiles_tx: &watch::Sender<Vec<StrategyProfile>>,
    enabled_strategy_tags_tx: &watch::Sender<HashSet<String>>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) {
    let _ = strategy_profiles_tx.send(strategy_catalog.profiles().to_vec());
    let _ = enabled_strategy_tags_tx.send(enabled_strategy_tags.clone());
    let _ = ws_instruments_tx.send(enabled_instruments(strategy_catalog, enabled_strategy_tags));
}

pub(super) fn sync_on_panel_selection(app_state: &mut AppState) {
    app_state.set_on_panel_selected(
        app_state
            .strategy_item_active
            .get(app_state.selected_grid_strategy_index())
            .copied()
            .unwrap_or(false),
    );
}

pub(super) fn open_grid_from_current_selection(app_state: &mut AppState, current_symbol: &str) {
    app_state.set_selected_grid_symbol_index(
        app_state
            .symbol_items
            .iter()
            .position(|item| item == current_symbol)
            .unwrap_or(0),
    );
    app_state.set_selected_grid_strategy_index(
        app_state
            .strategy_items
            .iter()
            .position(|item| item == &app_state.strategy_label)
            .unwrap_or(0),
    );
    sync_on_panel_selection(app_state);
    app_state.set_grid_open(true);
}

pub(super) fn grid_panel_indices(app_state: &AppState) -> Vec<usize> {
    app_state
        .strategy_item_active
        .iter()
        .enumerate()
        .filter_map(|(idx, active)| {
            if *active == app_state.is_on_panel_selected() {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn toggle_grid_panel_selection(app_state: &mut AppState) {
    app_state.set_on_panel_selected(!app_state.is_on_panel_selected());
    let panel_indices = grid_panel_indices(app_state);
    if !panel_indices.contains(&app_state.selected_grid_strategy_index()) {
        if let Some(first) = panel_indices.first().copied() {
            app_state.set_selected_grid_strategy_index(first);
        }
    }
}

pub(super) fn move_grid_strategy_selection(app_state: &mut AppState, up: bool) {
    let panel_indices = grid_panel_indices(app_state);
    if let Some(pos) = panel_indices
        .iter()
        .position(|idx| *idx == app_state.selected_grid_strategy_index())
    {
        let next_pos = if up {
            pos.saturating_sub(1)
        } else {
            (pos + 1).min(panel_indices.len().saturating_sub(1))
        };
        app_state.set_selected_grid_strategy_index(panel_indices[next_pos]);
    } else if let Some(first) = panel_indices.first().copied() {
        app_state.set_selected_grid_strategy_index(first);
    }
}

pub(super) fn refresh_and_sync_strategy_panel(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    enabled_strategy_tags: &HashSet<String>,
) {
    refresh_strategy_lists(app_state, strategy_catalog, enabled_strategy_tags);
    sync_on_panel_selection(app_state);
}

pub(super) fn persist_and_publish_strategy_state(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    current_strategy_profile: &StrategyProfile,
    enabled_strategy_tags: &HashSet<String>,
    strategy_profiles_tx: &watch::Sender<Vec<StrategyProfile>>,
    enabled_strategy_tags_tx: &watch::Sender<HashSet<String>>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) {
    persist_strategy_session_state(
        app_state,
        strategy_catalog,
        current_strategy_profile,
        enabled_strategy_tags,
    );
    publish_strategy_runtime_updates(
        strategy_catalog,
        enabled_strategy_tags,
        strategy_profiles_tx,
        enabled_strategy_tags_tx,
        ws_instruments_tx,
    );
}

pub(super) fn publish_and_persist_strategy_state(
    app_state: &mut AppState,
    strategy_catalog: &StrategyCatalog,
    current_strategy_profile: &StrategyProfile,
    enabled_strategy_tags: &HashSet<String>,
    strategy_profiles_tx: &watch::Sender<Vec<StrategyProfile>>,
    enabled_strategy_tags_tx: &watch::Sender<HashSet<String>>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) {
    publish_strategy_runtime_updates(
        strategy_catalog,
        enabled_strategy_tags,
        strategy_profiles_tx,
        enabled_strategy_tags_tx,
        ws_instruments_tx,
    );
    persist_strategy_session_state(
        app_state,
        strategy_catalog,
        current_strategy_profile,
        enabled_strategy_tags,
    );
}

pub(super) fn mark_strategy_running(strategy_catalog: &mut StrategyCatalog, source_tag: &str) {
    let _ = strategy_catalog.mark_running(source_tag, chrono::Utc::now().timestamp_millis());
}

pub(super) fn mark_strategy_stopped(strategy_catalog: &mut StrategyCatalog, source_tag: &str) {
    let _ = strategy_catalog.mark_stopped(source_tag, chrono::Utc::now().timestamp_millis());
}

pub(super) fn set_strategy_enabled(
    strategy_catalog: &mut StrategyCatalog,
    enabled_strategy_tags: &mut HashSet<String>,
    source_tag: &str,
    enabled: bool,
    paused: bool,
) {
    if enabled {
        enabled_strategy_tags.insert(source_tag.to_string());
        if !paused {
            mark_strategy_running(strategy_catalog, source_tag);
        }
    } else if enabled_strategy_tags.remove(source_tag) {
        mark_strategy_stopped(strategy_catalog, source_tag);
    }
}

pub(super) fn apply_symbol_selection(
    next_symbol: &str,
    current_symbol: &mut String,
    app_state: &mut AppState,
    ws_symbol_tx: &watch::Sender<String>,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    if next_symbol == current_symbol {
        return;
    }
    *current_symbol = next_symbol.to_string();
    app_state.symbol = current_symbol.clone();
    app_state.position = Position::new(current_symbol.clone());
    app_state.candles.clear();
    app_state.current_candle = None;
    app_state.fill_markers.clear();
    app_state.open_order_history.clear();
    app_state.filled_order_history.clear();
    app_state.history_fills.clear();
    app_state.last_applied_fee = "---".to_string();
    app_state.trade_stats_reset_warned = false;
    let _ = ws_symbol_tx.send(current_symbol.clone());
    switch_timeframe(
        current_symbol,
        &app_state.timeframe,
        rest_client,
        config,
        app_tx,
    );
    app_state.push_log(format!("Symbol switched to {}", current_symbol));
    let (_, market) = parse_instrument_label(current_symbol);
    if market == MarketKind::Futures {
        app_state.push_log("Futures mode enabled (orders + chart)".to_string());
    }
}

pub(super) fn handle_symbol_selector_popup_command(
    cmd: PopupCommand,
    app_state: &mut AppState,
    current_symbol: &mut String,
    ws_symbol_tx: &watch::Sender<String>,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
) {
    match cmd {
        PopupCommand::Close => {
            app_state.set_symbol_selector_open(false);
        }
        PopupCommand::Up => {
            app_state
                .set_symbol_selector_index(app_state.symbol_selector_index().saturating_sub(1));
        }
        PopupCommand::Down => {
            app_state.set_symbol_selector_index(
                (app_state.symbol_selector_index() + 1)
                    .min(app_state.symbol_items.len().saturating_sub(1)),
            );
        }
        PopupCommand::Confirm => {
            if let Some(next_symbol) = app_state
                .symbol_items
                .get(app_state.symbol_selector_index())
                .cloned()
            {
                apply_symbol_selection(
                    &next_symbol,
                    current_symbol,
                    app_state,
                    ws_symbol_tx,
                    rest_client,
                    config,
                    app_tx,
                );
            }
            app_state.set_symbol_selector_open(false);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_strategy_selector_popup_command(
    cmd: PopupCommand,
    app_state: &mut AppState,
    strategy_catalog: &mut StrategyCatalog,
    enabled_strategy_tags: &mut HashSet<String>,
    current_symbol: &mut String,
    current_strategy_profile: &mut StrategyProfile,
    ws_symbol_tx: &watch::Sender<String>,
    rest_client: &Arc<BinanceRestClient>,
    config: &Config,
    app_tx: &mpsc::Sender<AppEvent>,
    strategy_profile_tx: &watch::Sender<StrategyProfile>,
    strategy_profiles_tx: &watch::Sender<Vec<StrategyProfile>>,
    enabled_strategy_tags_tx: &watch::Sender<HashSet<String>>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) {
    match cmd {
        PopupCommand::Close => {
            app_state.set_strategy_selector_open(false);
        }
        PopupCommand::Up => {
            app_state
                .set_strategy_selector_index(app_state.strategy_selector_index().saturating_sub(1));
        }
        PopupCommand::Down => {
            app_state.set_strategy_selector_index(
                (app_state.strategy_selector_index() + 1)
                    .min(app_state.strategy_items.len().saturating_sub(1)),
            );
        }
        PopupCommand::Confirm => {
            if let Some(next_profile) = strategy_catalog
                .get(app_state.strategy_selector_index())
                .cloned()
            {
                set_strategy_enabled(
                    strategy_catalog,
                    enabled_strategy_tags,
                    &next_profile.source_tag,
                    true,
                    app_state.paused,
                );
                apply_symbol_selection(
                    &next_profile.symbol,
                    current_symbol,
                    app_state,
                    ws_symbol_tx,
                    rest_client,
                    config,
                    app_tx,
                );
                *current_strategy_profile = next_profile.clone();
                app_state.strategy_label = next_profile.label.clone();
                app_state.set_focus_strategy_id(Some(next_profile.label.clone()));
                refresh_strategy_lists(app_state, strategy_catalog, enabled_strategy_tags);
                app_state.set_selected_grid_strategy_index(
                    strategy_catalog
                        .index_of_label(&next_profile.label)
                        .unwrap_or(0),
                );
                app_state.fast_sma = None;
                app_state.slow_sma = None;
                let _ = strategy_profile_tx.send(next_profile.clone());
                publish_and_persist_strategy_state(
                    app_state,
                    strategy_catalog,
                    current_strategy_profile,
                    enabled_strategy_tags,
                    strategy_profiles_tx,
                    enabled_strategy_tags_tx,
                    ws_instruments_tx,
                );
                app_state.push_log(format!("Strategy selected: {} (ON)", next_profile.label));
            }
            app_state.set_strategy_selector_open(false);
        }
        _ => {}
    }
}
