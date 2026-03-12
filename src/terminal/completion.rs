#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCompletion {
    pub value: String,
    pub description: String,
}

pub fn format_completion_line(completions: &[ShellCompletion], selected: usize) -> String {
    completions
        .iter()
        .enumerate()
        .map(|(index, item)| {
            if index == selected {
                format!("[{}]", item.value)
            } else {
                item.value.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn scroll_lines_needed(current_row: u16, terminal_height: u16, lines_needed: usize) -> usize {
    if terminal_height == 0 {
        return 0;
    }

    let last_row = terminal_height.saturating_sub(1) as usize;
    let current_row = current_row as usize;
    current_row
        .saturating_add(lines_needed)
        .saturating_sub(last_row)
}

pub fn next_completion_index(len: usize, current: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

pub fn previous_completion_index(len: usize, current: usize) -> usize {
    if len == 0 {
        0
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}
