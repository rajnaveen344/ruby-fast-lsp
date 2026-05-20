use crate::core::{SourceFileId, TextRange};

#[derive(Debug, Clone)]
pub struct SourceDocument {
    content: String,
    file_id: SourceFileId,
    line_offsets: Vec<usize>,
    comments: Vec<(usize, usize)>,
}

impl SourceDocument {
    pub fn new(content: String, file_id: SourceFileId) -> Self {
        let line_offsets = compute_line_offsets(&content);
        let comments = parse_comments(&content);
        Self {
            content,
            file_id,
            line_offsets,
            comments,
        }
    }

    pub fn update(&mut self, content: String) {
        self.line_offsets = compute_line_offsets(&content);
        self.comments = parse_comments(&content);
        self.content = content;
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn file_id(&self) -> SourceFileId {
        self.file_id
    }

    pub fn set_file_id(&mut self, file_id: SourceFileId) {
        self.file_id = file_id;
    }

    pub fn comments(&self) -> &[(usize, usize)] {
        &self.comments
    }

    pub fn offset_to_line_character(&self, offset: usize) -> (u32, u32) {
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        let line_index = match self.line_offsets.binary_search(&offset) {
            Ok(exact) => exact,
            Err(after) => after - 1,
        };
        let line_start = self.line_offsets[line_index];
        let character = self.content[line_start..offset].chars().count();
        (
            u32::try_from(line_index).expect(
                "INVARIANT VIOLATED: source line index exceeded u32. \
                 This is a bug because editor protocol positions use u32. \
                 Fix: reject or segment files with more than u32::MAX lines.",
            ),
            u32::try_from(character).expect(
                "INVARIANT VIOLATED: source character index exceeded u32. \
                 This is a bug because editor protocol positions use u32. \
                 Fix: reject or segment lines with more than u32::MAX characters.",
            ),
        )
    }

    pub fn line_character_to_offset(&self, line: u32, character: u32) -> usize {
        let line = usize::try_from(line).expect(
            "INVARIANT VIOLATED: u32 line could not convert to usize. \
             This is a bug because usize must represent u32 on supported platforms. \
             Fix: unsupported target architecture.",
        );
        if line >= self.line_offsets.len() - 1 {
            return self.content.len();
        }

        let line_start = self.line_offsets[line];
        let line_end = self.line_offsets[line + 1];
        let target_char = usize::try_from(character).expect(
            "INVARIANT VIOLATED: u32 character could not convert to usize. \
             This is a bug because usize must represent u32 on supported platforms. \
             Fix: unsupported target architecture.",
        );

        let mut byte_offset = 0;
        for (chars_seen, c) in self.content[line_start..line_end].chars().enumerate() {
            if chars_seen >= target_char || c == '\n' {
                break;
            }
            byte_offset += c.len_utf8();
        }

        line_start + byte_offset
    }

    pub fn prism_location_to_text_range(&self, location: &ruby_prism::Location<'_>) -> TextRange {
        self.text_range_from_offsets(location.start_offset(), location.end_offset())
    }

    pub fn text_range_from_offsets(&self, start: usize, end: usize) -> TextRange {
        TextRange::new(self.file_id, u32_offset(start), u32_offset(end))
    }

    pub fn line_offsets(&self) -> &[usize] {
        &self.line_offsets
    }
}

fn compute_line_offsets(content: &str) -> Vec<usize> {
    let mut line_offsets = vec![0];
    let mut offset = 0;
    for c in content.chars() {
        offset += c.len_utf8();
        if c == '\n' {
            line_offsets.push(offset);
        }
    }
    if line_offsets.last() != Some(&content.len()) {
        line_offsets.push(content.len());
    }
    line_offsets
}

fn parse_comments(content: &str) -> Vec<(usize, usize)> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let mut comments = Vec::new();
    for comment in parse_result.comments() {
        let loc = comment.location();
        comments.push((loc.start_offset(), loc.end_offset()));
    }
    comments
}

fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect(
        "INVARIANT VIOLATED: source byte offset exceeded u32. \
         This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
         Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
    )
}

#[cfg(test)]
mod tests {
    use crate::core::SourceFileId;

    use super::*;

    #[test]
    fn computes_line_offsets() {
        let doc = SourceDocument::new(
            "def foo\n  puts 'Hello'\nend\n".to_string(),
            SourceFileId(7),
        );

        assert_eq!(doc.line_offsets(), &[0, 8, 23, 27]);
    }

    #[test]
    fn converts_offsets_and_line_characters() {
        let doc = SourceDocument::new("line1\nline2\nline3".to_string(), SourceFileId(0));

        assert_eq!(doc.offset_to_line_character(0), (0, 0));
        assert_eq!(doc.offset_to_line_character(6), (1, 0));
        assert_eq!(doc.line_character_to_offset(1, 3), 9);
        assert_eq!(doc.line_character_to_offset(100, 0), 17);
    }

    #[test]
    fn handles_utf8_character_offsets() {
        let doc = SourceDocument::new("hello 你好\nworld".to_string(), SourceFileId(0));

        assert_eq!(doc.offset_to_line_character(7), (0, 6));
        assert_eq!(doc.line_character_to_offset(0, 6), 6);
    }

    #[test]
    fn parses_comments() {
        let doc = SourceDocument::new("# hello\nclass User\nend\n".to_string(), SourceFileId(0));

        assert_eq!(doc.comments(), &[(0, 7)]);
    }
}
