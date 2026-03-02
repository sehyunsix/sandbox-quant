use super::*;

pub(super) fn handle_account_popup_command(cmd: PopupCommand, app_state: &mut AppState) {
    if cmd == PopupCommand::Close {
        app_state.set_account_popup_open(false);
    }
}

pub(super) fn handle_history_popup_command(cmd: PopupCommand, app_state: &mut AppState) {
    match cmd {
        PopupCommand::HistoryDay => {
            app_state.history_bucket = order_store::HistoryBucket::Day;
            app_state.refresh_history_rows();
        }
        PopupCommand::HistoryHour => {
            app_state.history_bucket = order_store::HistoryBucket::Hour;
            app_state.refresh_history_rows();
        }
        PopupCommand::HistoryMonth => {
            app_state.history_bucket = order_store::HistoryBucket::Month;
            app_state.refresh_history_rows();
        }
        PopupCommand::Close => {
            app_state.set_history_popup_open(false);
        }
        _ => {}
    }
}

pub(super) fn handle_focus_popup_command(cmd: PopupCommand, app_state: &mut AppState) {
    if cmd == PopupCommand::Close {
        app_state.set_focus_popup_open(false);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_strategy_editor_key(
    key_code: &KeyCode,
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
    let editor_kind = |idx: usize, items: &[String]| {
        items
            .get(idx)
            .and_then(|name| StrategyKind::from_label(name))
            .unwrap_or(StrategyKind::Ma)
    };
    let apply_editor_kind_defaults = |app_state: &mut AppState| {
        let kind = editor_kind(
            app_state.strategy_editor_kind_index,
            &app_state.strategy_editor_kind_items,
        );
        let (fast, slow, cooldown) = kind.defaults();
        app_state.strategy_editor_fast = fast;
        app_state.strategy_editor_slow = slow;
        app_state.strategy_editor_cooldown = app_state.strategy_editor_cooldown.max(cooldown);
    };

    match key_code {
        KeyCode::Esc => {
            if app_state.strategy_editor_kind_category_selector_open {
                app_state.strategy_editor_kind_category_selector_open = false;
                return;
            }
            if app_state.strategy_editor_kind_selector_open {
                app_state.strategy_editor_kind_selector_open = false;
                app_state.strategy_editor_kind_popup_items.clear();
                app_state.strategy_editor_kind_popup_labels.clear();
                return;
            }
            app_state.strategy_editor_kind_category_selector_open = false;
            app_state.strategy_editor_kind_selector_open = false;
            app_state.strategy_editor_kind_popup_items.clear();
            app_state.strategy_editor_kind_popup_labels.clear();
            app_state.set_strategy_editor_open(false);
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K')
            if app_state.strategy_editor_kind_category_selector_open =>
        {
            app_state.strategy_editor_kind_category_index = app_state
                .strategy_editor_kind_category_index
                .saturating_sub(1)
                .min(
                    app_state
                        .strategy_editor_kind_category_items
                        .len()
                        .saturating_sub(1),
                );
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J')
            if app_state.strategy_editor_kind_category_selector_open =>
        {
            app_state.strategy_editor_kind_category_index =
                (app_state.strategy_editor_kind_category_index + 1).min(
                    app_state
                        .strategy_editor_kind_category_items
                        .len()
                        .saturating_sub(1),
                );
        }
        KeyCode::Enter if app_state.strategy_editor_kind_category_selector_open => {
            let selected_category = app_state
                .strategy_editor_kind_category_items
                .get(app_state.strategy_editor_kind_category_index)
                .cloned()
                .unwrap_or_else(|| "Trend".to_string());
            let options = strategy_type_options_by_category(&selected_category);
            app_state.strategy_editor_kind_popup_items = options
                .iter()
                .map(|item| item.display_label.clone())
                .collect();
            app_state.strategy_editor_kind_popup_labels = options
                .iter()
                .map(|item| item.strategy_label.clone())
                .collect();
            let current_label = app_state
                .strategy_editor_kind_items
                .get(app_state.strategy_editor_kind_index)
                .cloned()
                .unwrap_or_else(|| "MA".to_string());
            app_state.strategy_editor_kind_selector_index = app_state
                .strategy_editor_kind_popup_labels
                .iter()
                .position(|item| {
                    item.as_ref()
                        .map(|label| label.eq_ignore_ascii_case(&current_label))
                        .unwrap_or(false)
                })
                .unwrap_or(0);
            app_state.strategy_editor_kind_category_selector_open = false;
            app_state.strategy_editor_kind_selector_open = true;
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K')
            if app_state.strategy_editor_kind_selector_open =>
        {
            app_state.strategy_editor_kind_selector_index = app_state
                .strategy_editor_kind_selector_index
                .saturating_sub(1)
                .min(
                    app_state
                        .strategy_editor_kind_popup_items
                        .len()
                        .saturating_sub(1),
                );
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J')
            if app_state.strategy_editor_kind_selector_open =>
        {
            app_state.strategy_editor_kind_selector_index =
                (app_state.strategy_editor_kind_selector_index + 1).min(
                    app_state
                        .strategy_editor_kind_popup_items
                        .len()
                        .saturating_sub(1),
                );
        }
        KeyCode::Enter if app_state.strategy_editor_kind_selector_open => {
            let popup_idx = app_state.strategy_editor_kind_selector_index.min(
                app_state
                    .strategy_editor_kind_popup_items
                    .len()
                    .saturating_sub(1),
            );
            if let Some(Some(selected_label)) = app_state
                .strategy_editor_kind_popup_labels
                .get(popup_idx)
                .cloned()
            {
                app_state.strategy_editor_kind_index = app_state
                    .strategy_editor_kind_items
                    .iter()
                    .position(|item| item.eq_ignore_ascii_case(&selected_label))
                    .unwrap_or(0);
                apply_editor_kind_defaults(app_state);
                app_state.strategy_editor_kind_selector_open = false;
                app_state.strategy_editor_kind_popup_items.clear();
                app_state.strategy_editor_kind_popup_labels.clear();
            } else if let Some(item) = app_state.strategy_editor_kind_popup_items.get(popup_idx) {
                app_state.push_log(format!("Strategy type not implemented yet: {}", item));
            }
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app_state.strategy_editor_field = app_state.strategy_editor_field.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app_state.strategy_editor_field = (app_state.strategy_editor_field + 1).min(4);
        }
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
            match app_state.strategy_editor_field {
                0 => {}
                1 => {
                    app_state.strategy_editor_symbol_index =
                        app_state.strategy_editor_symbol_index.saturating_sub(1)
                }
                2 => {
                    app_state.strategy_editor_fast =
                        app_state.strategy_editor_fast.saturating_sub(1).max(2)
                }
                3 => {
                    app_state.strategy_editor_slow =
                        app_state.strategy_editor_slow.saturating_sub(1).max(3)
                }
                _ => {
                    app_state.strategy_editor_cooldown =
                        app_state.strategy_editor_cooldown.saturating_sub(1).max(1)
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
            match app_state.strategy_editor_field {
                0 => {}
                1 => {
                    app_state.strategy_editor_symbol_index =
                        (app_state.strategy_editor_symbol_index + 1)
                            .min(app_state.symbol_items.len().saturating_sub(1))
                }
                2 => app_state.strategy_editor_fast += 1,
                3 => app_state.strategy_editor_slow += 1,
                _ => app_state.strategy_editor_cooldown += 1,
            }
        }
        KeyCode::Enter => {
            if app_state.strategy_editor_field == 0 {
                let current_label = app_state
                    .strategy_editor_kind_items
                    .get(app_state.strategy_editor_kind_index)
                    .cloned()
                    .unwrap_or_else(|| "MA".to_string());
                app_state.strategy_editor_kind_category_index =
                    strategy_kind_category_for_label(&current_label)
                        .and_then(|category| {
                            app_state
                                .strategy_editor_kind_category_items
                                .iter()
                                .position(|item| item.eq_ignore_ascii_case(&category))
                        })
                        .unwrap_or(0);
                app_state.strategy_editor_kind_popup_items.clear();
                app_state.strategy_editor_kind_popup_labels.clear();
                app_state.strategy_editor_kind_category_selector_open = true;
                app_state.strategy_editor_kind_selector_open = false;
                app_state.strategy_editor_kind_selector_index = app_state
                    .strategy_editor_kind_index
                    .min(app_state.strategy_editor_kind_items.len().saturating_sub(1));
                return;
            }
            let edited_profile = strategy_catalog
                .get(app_state.strategy_editor_index)
                .cloned();
            let selected_kind = editor_kind(
                app_state.strategy_editor_kind_index,
                &app_state.strategy_editor_kind_items,
            );
            let selected_symbol = app_state
                .symbol_items
                .get(app_state.strategy_editor_symbol_index)
                .cloned()
                .unwrap_or_else(|| app_state.symbol.clone());
            let maybe_updated = strategy_catalog.fork_profile(
                app_state.strategy_editor_index,
                selected_kind,
                &selected_symbol,
                app_state.strategy_editor_fast,
                app_state.strategy_editor_slow,
                app_state.strategy_editor_cooldown,
            );
            if let Some(updated) = maybe_updated {
                refresh_strategy_lists(app_state, strategy_catalog, enabled_strategy_tags);
                app_state.set_selected_grid_strategy_index(
                    strategy_catalog.index_of_label(&updated.label).unwrap_or(0),
                );
                if edited_profile.as_ref().map(|p| p.source_tag.as_str())
                    == Some(current_strategy_profile.source_tag.as_str())
                {
                    set_strategy_enabled(
                        strategy_catalog,
                        enabled_strategy_tags,
                        &current_strategy_profile.source_tag,
                        false,
                        app_state.paused,
                    );
                    set_strategy_enabled(
                        strategy_catalog,
                        enabled_strategy_tags,
                        &updated.source_tag,
                        true,
                        app_state.paused,
                    );
                    *current_strategy_profile = updated.clone();
                    app_state.strategy_label = updated.label.clone();
                    app_state.set_focus_strategy_id(Some(updated.label.clone()));
                    apply_symbol_selection(
                        &updated.symbol,
                        current_symbol,
                        app_state,
                        ws_symbol_tx,
                        rest_client,
                        config,
                        app_tx,
                    );
                    app_state.fast_sma = None;
                    app_state.slow_sma = None;
                    let _ = strategy_profile_tx.send(updated.clone());
                }
                if let Some(before) = edited_profile.as_ref() {
                    app_state.push_log(format!(
                        "Strategy forked: {} -> {}",
                        before.label, updated.label
                    ));
                } else {
                    app_state.push_log(format!("Strategy forked: {}", updated.label));
                }
                persist_and_publish_strategy_state(
                    app_state,
                    strategy_catalog,
                    current_strategy_profile,
                    enabled_strategy_tags,
                    strategy_profiles_tx,
                    enabled_strategy_tags_tx,
                    ws_instruments_tx,
                );
            }
            app_state.strategy_editor_kind_category_selector_open = false;
            app_state.strategy_editor_kind_selector_open = false;
            app_state.strategy_editor_kind_popup_items.clear();
            app_state.strategy_editor_kind_popup_labels.clear();
            app_state.set_strategy_editor_open(false);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_grid_strategy_action(
    cmd: GridCommand,
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
    strategy_enabled_tx: &watch::Sender<bool>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) -> bool {
    match cmd {
        GridCommand::NewStrategy => {
            let selected_index = app_state
                .selected_grid_strategy_index()
                .min(strategy_catalog.len().saturating_sub(1));
            app_state.set_strategy_editor_open(true);
            app_state.strategy_editor_kind_category_selector_open = false;
            app_state.strategy_editor_kind_selector_open = false;
            app_state.strategy_editor_kind_popup_items.clear();
            app_state.strategy_editor_kind_popup_labels.clear();
            app_state.strategy_editor_index = selected_index;
            app_state.strategy_editor_field = 0;
            if let Some(base_profile) = strategy_catalog.get(selected_index) {
                app_state.strategy_editor_kind_index = app_state
                    .strategy_editor_kind_items
                    .iter()
                    .position(|item| {
                        item.eq_ignore_ascii_case(base_profile.strategy_kind().as_label())
                    })
                    .unwrap_or(0);
                app_state.strategy_editor_symbol_index = app_state
                    .symbol_items
                    .iter()
                    .position(|item| item == &base_profile.symbol)
                    .unwrap_or(0);
                app_state.strategy_editor_fast = base_profile.fast_period;
                app_state.strategy_editor_slow = base_profile.slow_period;
                app_state.strategy_editor_cooldown = base_profile.min_ticks_between_signals;
            }
            app_state.push_log("New strategy draft opened".to_string());
            true
        }
        GridCommand::EditStrategyConfig => {
            if let Some(selected_label) = app_state
                .strategy_items
                .get(app_state.selected_grid_strategy_index())
                .cloned()
            {
                if let Some(idx) = strategy_catalog.index_of_label(&selected_label) {
                    if let Some(profile) = strategy_catalog.get(idx).cloned() {
                        app_state.set_strategy_editor_open(true);
                        app_state.strategy_editor_kind_category_selector_open = false;
                        app_state.strategy_editor_kind_selector_open = false;
                        app_state.strategy_editor_kind_popup_items.clear();
                        app_state.strategy_editor_kind_popup_labels.clear();
                        app_state.strategy_editor_index = idx;
                        app_state.strategy_editor_field = 0;
                        app_state.strategy_editor_kind_index = app_state
                            .strategy_editor_kind_items
                            .iter()
                            .position(|item| {
                                item.eq_ignore_ascii_case(profile.strategy_kind().as_label())
                            })
                            .unwrap_or(0);
                        app_state.strategy_editor_symbol_index = app_state
                            .symbol_items
                            .iter()
                            .position(|item| item == &profile.symbol)
                            .unwrap_or(0);
                        app_state.strategy_editor_fast = profile.fast_period;
                        app_state.strategy_editor_slow = profile.slow_period;
                        app_state.strategy_editor_cooldown = profile.min_ticks_between_signals;
                    }
                }
            }
            true
        }
        GridCommand::DeleteStrategy => {
            let selected_idx = app_state
                .selected_grid_strategy_index()
                .min(strategy_catalog.len().saturating_sub(1));
            let selected_profile = strategy_catalog.get(selected_idx).cloned();
            if let Some(profile) = selected_profile {
                if !StrategyCatalog::is_custom_source_tag(&profile.source_tag) {
                    app_state.push_log(format!(
                        "Strategy delete blocked (builtin): {}",
                        profile.label
                    ));
                } else if strategy_catalog
                    .remove_custom_profile(selected_idx)
                    .is_some()
                {
                    enabled_strategy_tags.remove(&profile.source_tag);
                    let mut fallback_idx = selected_idx.saturating_sub(1);
                    if strategy_catalog.len() > 0 {
                        fallback_idx = fallback_idx.min(strategy_catalog.len().saturating_sub(1));
                    }
                    if profile.source_tag == current_strategy_profile.source_tag {
                        set_strategy_enabled(
                            strategy_catalog,
                            enabled_strategy_tags,
                            &current_strategy_profile.source_tag,
                            false,
                            app_state.paused,
                        );
                        if let Some(next_profile) = strategy_catalog.get(fallback_idx).cloned() {
                            set_strategy_enabled(
                                strategy_catalog,
                                enabled_strategy_tags,
                                &next_profile.source_tag,
                                true,
                                app_state.paused,
                            );
                            *current_strategy_profile = next_profile.clone();
                            app_state.strategy_label = next_profile.label.clone();
                            app_state.set_focus_strategy_id(Some(next_profile.label.clone()));
                            apply_symbol_selection(
                                &next_profile.symbol,
                                current_symbol,
                                app_state,
                                ws_symbol_tx,
                                rest_client,
                                config,
                                app_tx,
                            );
                            app_state.fast_sma = None;
                            app_state.slow_sma = None;
                            let _ = strategy_profile_tx.send(next_profile);
                        }
                    }
                    publish_strategy_runtime_updates(
                        strategy_catalog,
                        enabled_strategy_tags,
                        strategy_profiles_tx,
                        enabled_strategy_tags_tx,
                        ws_instruments_tx,
                    );
                    refresh_and_sync_strategy_panel(
                        app_state,
                        strategy_catalog,
                        enabled_strategy_tags,
                    );
                    app_state.set_selected_grid_strategy_index(
                        fallback_idx.min(strategy_catalog.len().saturating_sub(1)),
                    );
                    app_state.push_log(format!("Strategy deleted: {}", profile.label));
                    persist_strategy_session_state(
                        app_state,
                        strategy_catalog,
                        current_strategy_profile,
                        enabled_strategy_tags,
                    );
                }
            }
            true
        }
        GridCommand::ToggleStrategyOnOff => {
            if let Some(item) = app_state
                .strategy_items
                .get(app_state.selected_grid_strategy_index())
                .cloned()
            {
                if let Some(next_profile) = strategy_catalog
                    .index_of_label(&item)
                    .and_then(|idx| strategy_catalog.get(idx).cloned())
                {
                    if enabled_strategy_tags.contains(&next_profile.source_tag) {
                        set_strategy_enabled(
                            strategy_catalog,
                            enabled_strategy_tags,
                            &next_profile.source_tag,
                            false,
                            app_state.paused,
                        );
                        app_state.push_log(format!("Strategy OFF: {}", next_profile.label));
                    } else {
                        set_strategy_enabled(
                            strategy_catalog,
                            enabled_strategy_tags,
                            &next_profile.source_tag,
                            true,
                            app_state.paused,
                        );
                        app_state.paused = false;
                        let _ = strategy_enabled_tx.send(true);
                        app_state
                            .push_log(format!("Strategy ON from grid: {}", next_profile.label));
                    }
                    publish_and_persist_strategy_state(
                        app_state,
                        strategy_catalog,
                        current_strategy_profile,
                        enabled_strategy_tags,
                        strategy_profiles_tx,
                        enabled_strategy_tags_tx,
                        ws_instruments_tx,
                    );
                    refresh_and_sync_strategy_panel(
                        app_state,
                        strategy_catalog,
                        enabled_strategy_tags,
                    );
                }
            }
            true
        }
        GridCommand::ActivateStrategy => {
            if let Some(item) = app_state
                .strategy_items
                .get(app_state.selected_grid_strategy_index())
                .cloned()
            {
                app_state.set_focus_symbol(Some(app_state.symbol.clone()));
                app_state.set_focus_strategy_id(Some(item.clone()));
                if let Some(next_profile) = strategy_catalog
                    .index_of_label(&item)
                    .and_then(|idx| strategy_catalog.get(idx).cloned())
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
                    app_state.push_log(format!(
                        "Strategy selected from grid: {} (ON)",
                        next_profile.label
                    ));
                }
                app_state.set_grid_open(false);
                app_state.set_focus_popup_open(false);
            }
            true
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_grid_key(
    key_code: &KeyCode,
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
    strategy_enabled_tx: &watch::Sender<bool>,
    ws_instruments_tx: &watch::Sender<Vec<String>>,
) {
    if let Some(cmd) = parse_grid_command(key_code) {
        match cmd {
            GridCommand::TabAssets => app_state.set_grid_tab(GridTab::Assets),
            GridCommand::TabStrategies => app_state.set_grid_tab(GridTab::Strategies),
            GridCommand::TabRisk => app_state.set_grid_tab(GridTab::Risk),
            GridCommand::TabNetwork => app_state.set_grid_tab(GridTab::Network),
            GridCommand::TabHistory => app_state.set_grid_tab(GridTab::History),
            GridCommand::TabPositions => app_state.set_grid_tab(GridTab::Positions),
            GridCommand::TabPredictors => app_state.set_grid_tab(GridTab::Predictors),
            GridCommand::TabSystemLog => app_state.set_grid_tab(GridTab::SystemLog),
            GridCommand::CloseGrid => app_state.set_grid_open(false),
            GridCommand::ToggleOnOffPanel => {
                if app_state.grid_tab() == GridTab::Strategies {
                    toggle_grid_panel_selection(app_state);
                }
            }
            GridCommand::StrategyUp => {
                if app_state.grid_tab() == GridTab::Strategies {
                    move_grid_strategy_selection(app_state, true);
                } else if app_state.grid_tab() == GridTab::Predictors {
                    app_state.set_predictor_scroll_offset(
                        app_state.predictor_scroll_offset().saturating_sub(1),
                    );
                }
            }
            GridCommand::StrategyDown => {
                if app_state.grid_tab() == GridTab::Strategies {
                    move_grid_strategy_selection(app_state, false);
                } else if app_state.grid_tab() == GridTab::Predictors {
                    let predictor_rows = app_state
                        .predictor_metrics_by_scope
                        .values()
                        .filter(|e| !app_state.hide_empty_predictor_rows || e.sample_count > 0)
                        .count();
                    if predictor_rows > 0 {
                        app_state.set_predictor_scroll_offset(
                            (app_state.predictor_scroll_offset() + 1).min(predictor_rows - 1),
                        );
                    } else {
                        app_state.set_predictor_scroll_offset(0);
                    }
                }
            }
            GridCommand::SymbolLeft => {
                if app_state.grid_tab() == GridTab::Strategies {
                    app_state.set_selected_grid_symbol_index(
                        app_state.selected_grid_symbol_index().saturating_sub(1),
                    );
                }
            }
            GridCommand::SymbolRight => {
                if app_state.grid_tab() == GridTab::Strategies {
                    app_state.set_selected_grid_symbol_index(
                        (app_state.selected_grid_symbol_index() + 1)
                            .min(app_state.symbol_items.len().saturating_sub(1)),
                    );
                }
            }
            GridCommand::ToggleSmallPositionsFilter => {
                if app_state.grid_tab() == GridTab::Positions {
                    app_state.hide_small_positions = !app_state.hide_small_positions;
                    app_state.push_log(format!(
                        "Positions <$1 filter: {}",
                        if app_state.hide_small_positions {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ));
                } else if app_state.grid_tab() == GridTab::Predictors {
                    app_state.hide_empty_predictor_rows = !app_state.hide_empty_predictor_rows;
                    app_state.set_predictor_scroll_offset(0);
                    app_state.push_log(format!(
                        "Predictors N=0 filter: {}",
                        if app_state.hide_empty_predictor_rows {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ));
                }
            }
            GridCommand::NewStrategy
            | GridCommand::EditStrategyConfig
            | GridCommand::DeleteStrategy
            | GridCommand::ToggleStrategyOnOff
            | GridCommand::ActivateStrategy => {
                if app_state.grid_tab() == GridTab::Strategies {
                    let _ = handle_grid_strategy_action(
                        cmd,
                        app_state,
                        strategy_catalog,
                        enabled_strategy_tags,
                        current_symbol,
                        current_strategy_profile,
                        ws_symbol_tx,
                        rest_client,
                        config,
                        app_tx,
                        strategy_profile_tx,
                        strategy_profiles_tx,
                        enabled_strategy_tags_tx,
                        strategy_enabled_tx,
                        ws_instruments_tx,
                    );
                }
            }
        }
    }
}
