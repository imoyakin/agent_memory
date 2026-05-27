fn key_value_table(value: &Value) -> String {
    if let Some(object) = value.as_object() {
        let rows: Vec<_> = object
            .iter()
            .map(|(key, value)| (key.as_str(), value_text(value)))
            .collect();
        key_value_rows(&rows)
    } else {
        value_text(value)
    }
}

fn key_value_rows(rows: &[(&str, String)]) -> String {
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| vec![(*key).to_string(), value.clone()])
        .collect();
    markdown_table(&["Field", "Value"], &table_rows)
}

fn markdown_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| display_len(header)).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(display_len(&truncate_cell(cell)));
            }
        }
    }
    let mut output = String::new();
    output.push('|');
    for (index, header) in headers.iter().enumerate() {
        output.push(' ');
        output.push_str(&pad_cell(header, widths[index]));
        output.push_str(" |");
    }
    output.push('\n');
    output.push('|');
    for width in &widths {
        output.push(' ');
        output.push_str(&"-".repeat(*width));
        output.push_str(" |");
    }
    output.push('\n');
    for row in rows {
        output.push('|');
        for (index, width) in widths.iter().enumerate().take(headers.len()) {
            let cell = row.get(index).map(String::as_str).unwrap_or("");
            output.push(' ');
            output.push_str(&pad_cell(&truncate_cell(cell), *width));
            output.push_str(" |");
        }
        output.push('\n');
    }
    output.trim_end().to_string()
}

fn pad_cell(value: &str, width: usize) -> String {
    let mut output = value.to_string();
    while display_len(&output) < width {
        output.push(' ');
    }
    output
}

fn truncate_cell(value: &str) -> String {
    const MAX: usize = 80;
    if display_len(value) <= MAX {
        value.to_string()
    } else {
        let mut output: String = value.chars().take(MAX.saturating_sub(3)).collect();
        output.push_str("...");
        output
    }
}

fn display_len(value: &str) -> usize {
    value.chars().count()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(items) => items.iter().map(value_text).collect::<Vec<_>>().join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}
