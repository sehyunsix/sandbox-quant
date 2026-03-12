use std::io::{self, Write};

use crossterm::cursor::{
    position, MoveToColumn, MoveToNextLine, MoveUp, RestorePosition, SavePosition,
};
use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, Clear, ClearType, ScrollUp};

use crate::terminal::app::{TerminalApp, TerminalEvent, TerminalMode};
use crate::terminal::completion::{
    next_completion_index, previous_completion_index, scroll_lines_needed, ShellCompletion,
};

pub fn run_terminal<A: TerminalApp>(app: &mut A) -> Result<(), Box<dyn std::error::Error>> {
    let intro_panel = app.intro_panel();
    execute!(
        io::stdout(),
        PrintStyledContent(intro_panel.cyan().bold()),
        Print("\n"),
        PrintStyledContent(app.help_heading().dark_grey()),
        Print("\n"),
        Print(app.help_text()),
        Print("\n")
    )?;

    match app.terminal_mode() {
        TerminalMode::Raw => {
            enable_raw_mode()?;
            let result = loop_terminal_raw(app);
            disable_raw_mode()?;
            result
        }
        TerminalMode::Line => loop_terminal_line(app),
    }
}

fn loop_terminal_raw<A: TerminalApp>(app: &mut A) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();
    let mut buffer = String::new();
    let mut completion_index = 0usize;
    let mut rendered_menu_lines = 0usize;
    let mut completion_query: Option<String> = None;
    render_shell(
        &mut stdout,
        app,
        &buffer,
        completion_index,
        &mut rendered_menu_lines,
    )?;

    loop {
        if let Event::Key(key) = read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    println!();
                    break;
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    completion_index = 0;
                    completion_query = Some(buffer.clone());
                    render_shell(
                        &mut stdout,
                        app,
                        &buffer,
                        completion_index,
                        &mut rendered_menu_lines,
                    )?;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    completion_index = 0;
                    completion_query = Some(buffer.clone());
                    render_shell(
                        &mut stdout,
                        app,
                        &buffer,
                        completion_index,
                        &mut rendered_menu_lines,
                    )?;
                }
                KeyCode::Tab => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = app.complete(&query);
                    if completions.len() == 1 {
                        buffer = completions[0].value.clone();
                        completion_index = 0;
                        completion_query = Some(query);
                        render_shell(
                            &mut stdout,
                            app,
                            &buffer,
                            completion_index,
                            &mut rendered_menu_lines,
                        )?;
                    } else if !completions.is_empty() {
                        completion_index = (completion_index + 1) % completions.len();
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(
                            &mut stdout,
                            app,
                            &buffer,
                            completion_index,
                            &mut rendered_menu_lines,
                        )?;
                    }
                }
                KeyCode::Up => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = app.complete(&query);
                    if !completions.is_empty() {
                        completion_index =
                            previous_completion_index(completions.len(), completion_index);
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(
                            &mut stdout,
                            app,
                            &buffer,
                            completion_index,
                            &mut rendered_menu_lines,
                        )?;
                    }
                }
                KeyCode::Down => {
                    let query = completion_query.clone().unwrap_or_else(|| buffer.clone());
                    let completions = app.complete(&query);
                    if !completions.is_empty() {
                        completion_index =
                            next_completion_index(completions.len(), completion_index);
                        buffer = completions[completion_index].value.clone();
                        completion_query = Some(query);
                        render_shell(
                            &mut stdout,
                            app,
                            &buffer,
                            completion_index,
                            &mut rendered_menu_lines,
                        )?;
                    }
                }
                KeyCode::Enter => {
                    clear_completion_menu(&mut stdout, rendered_menu_lines)?;
                    rendered_menu_lines = 0;
                    println!();
                    let line = buffer.clone();
                    buffer.clear();
                    completion_index = 0;
                    completion_query = None;
                    match app.execute_line(&line) {
                        Ok(TerminalEvent::NoOutput) => {}
                        Ok(TerminalEvent::Output(output)) => {
                            print_multiline_block(&mut stdout, &output, true)?
                        }
                        Ok(TerminalEvent::Exit) => break,
                        Err(error) => print_error(&mut stdout, error)?,
                    }
                    render_shell(
                        &mut stdout,
                        app,
                        &buffer,
                        completion_index,
                        &mut rendered_menu_lines,
                    )?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn loop_terminal_line<A: TerminalApp>(app: &mut A) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("{}", app.prompt());
        stdout.flush()?;

        let mut line = String::new();
        stdin.read_line(&mut line)?;
        match app.execute_line(&line) {
            Ok(TerminalEvent::NoOutput) => {}
            Ok(TerminalEvent::Output(output)) => {
                print_multiline_block(&mut stdout, &output, true)?;
            }
            Ok(TerminalEvent::Exit) => break,
            Err(error) => print_error(&mut stdout, error)?,
        }
    }

    Ok(())
}

fn render_prompt<A: TerminalApp>(stdout: &mut io::Stdout, app: &A, buffer: &str) -> io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    execute!(stdout, Print(app.prompt()), Print(buffer))?;
    stdout.flush()
}

fn render_shell<A: TerminalApp>(
    stdout: &mut io::Stdout,
    app: &A,
    buffer: &str,
    completion_index: usize,
    rendered_menu_lines: &mut usize,
) -> io::Result<()> {
    let completions = app.complete(buffer);
    let expected_menu_lines = if should_show_completion_menu(buffer, &completions) {
        completions.len() + 1
    } else {
        0
    };
    let scrolled = ensure_vertical_space(stdout, expected_menu_lines)?;
    if scrolled > 0 {
        execute!(stdout, MoveUp(scrolled as u16))?;
    }
    render_prompt(stdout, app, buffer)?;
    execute!(stdout, SavePosition)?;
    let menu_lines = if should_show_completion_menu(buffer, &completions) {
        print_completion_menu(stdout, &completions, completion_index)?
    } else {
        0
    };

    let lines_to_clear = (*rendered_menu_lines).saturating_sub(menu_lines);
    for _ in 0..lines_to_clear {
        execute!(
            stdout,
            MoveToNextLine(1),
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
    }

    *rendered_menu_lines = menu_lines;
    execute!(stdout, RestorePosition)?;
    stdout.flush()
}

fn clear_completion_menu(stdout: &mut io::Stdout, rendered_menu_lines: usize) -> io::Result<()> {
    execute!(stdout, SavePosition)?;
    for _ in 0..rendered_menu_lines {
        execute!(
            stdout,
            MoveToNextLine(1),
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
    }
    execute!(stdout, RestorePosition)?;
    stdout.flush()
}

fn print_completion_menu(
    stdout: &mut io::Stdout,
    completions: &[ShellCompletion],
    selected: usize,
) -> io::Result<usize> {
    execute!(
        stdout,
        MoveToNextLine(1),
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        PrintStyledContent("completions".dark_grey()),
    )?;

    for (index, item) in completions.iter().enumerate() {
        execute!(
            stdout,
            MoveToNextLine(1),
            MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
        if index == selected {
            execute!(
                stdout,
                PrintStyledContent(">".cyan().bold()),
                Print(" "),
                PrintStyledContent(item.value.as_str().black().on_white()),
                Print("  "),
                PrintStyledContent(item.description.as_str().dark_grey()),
            )?;
        } else {
            execute!(
                stdout,
                Print("  "),
                PrintStyledContent(item.value.as_str().dark_grey()),
                Print("  "),
                PrintStyledContent(item.description.as_str().dark_grey()),
            )?;
        }
    }
    Ok(completions.len() + 1)
}

fn print_error(stdout: &mut io::Stdout, error: impl std::fmt::Display) -> io::Result<()> {
    print_multiline_block(stdout, &format!("error: {error}"), false)
}

fn print_multiline_block(stdout: &mut io::Stdout, text: &str, cyan_output: bool) -> io::Result<()> {
    for (index, line) in text.lines().enumerate() {
        if index == 0 {
            begin_output_block(stdout)?;
        } else {
            execute!(stdout, MoveToColumn(0))?;
        }

        if cyan_output {
            writeln!(stdout, "{}", line.cyan())?;
        } else if let Some(rest) = line.strip_prefix("error: ") {
            writeln!(stdout, "{} {}", "error:".red().bold(), rest.red())?;
        } else {
            writeln!(stdout, "{line}")?;
        }
    }
    Ok(())
}

fn begin_output_block(stdout: &mut io::Stdout) -> io::Result<()> {
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    Ok(())
}

fn should_show_completion_menu(buffer: &str, completions: &[ShellCompletion]) -> bool {
    buffer.trim_start().starts_with('/') && !completions.is_empty()
}

fn ensure_vertical_space(stdout: &mut io::Stdout, lines_needed: usize) -> io::Result<usize> {
    if lines_needed == 0 {
        return Ok(0);
    }

    let Ok((_, row)) = position() else {
        return Ok(0);
    };
    let Ok((_, height)) = size() else {
        return Ok(0);
    };
    let overflow = scroll_lines_needed(row, height, lines_needed);
    if overflow > 0 {
        execute!(stdout, ScrollUp(overflow as u16))?;
    }
    Ok(overflow)
}
