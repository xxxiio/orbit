use crate::app;

/// Byte offset for a character index, clamped to the value length.
pub(crate) fn byte_offset(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(offset, _)| offset)
        .unwrap_or(value.len())
}

/// Insert a character at the caret.
pub(crate) fn text_insert(field: &mut app::FormField, character: char) {
    let offset = byte_offset(&field.value, field.cursor);
    field.value.insert(offset, character);
    field.cursor += 1;
}

/// Delete the character before the caret.
pub(crate) fn text_backspace(field: &mut app::FormField) {
    if field.cursor == 0 {
        return;
    }
    let end = byte_offset(&field.value, field.cursor);
    let start = byte_offset(&field.value, field.cursor - 1);
    field.value.replace_range(start..end, "");
    field.cursor -= 1;
}

/// Delete the character at the caret.
pub(crate) fn text_delete(field: &mut app::FormField) {
    let len = field.value.chars().count();
    if field.cursor >= len {
        return;
    }
    let start = byte_offset(&field.value, field.cursor);
    let end = byte_offset(&field.value, field.cursor + 1);
    field.value.replace_range(start..end, "");
}
