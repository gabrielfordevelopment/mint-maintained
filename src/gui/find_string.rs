use egui::{Color32, TextFormat, text::LayoutJob};

pub(crate) struct FindString<'data> {
    chunks: std::vec::IntoIter<(bool, &'data str)>,
}

impl<'data> FindString<'data> {
    pub(crate) fn new(string: &'data str, needle: &'data str) -> Self {
        let mut folded = String::new();
        let mut ranges = Vec::new();
        for (start, character) in string.char_indices() {
            for lower in character.to_lowercase() {
                folded.push(lower);
                ranges.extend(std::iter::repeat_n(
                    (start, start + character.len_utf8()),
                    lower.len_utf8(),
                ));
            }
        }
        let needle: String = needle.chars().flat_map(char::to_lowercase).collect();
        let mut chunks = Vec::new();
        let mut end = 0;
        if !needle.is_empty() {
            for (index, _) in folded.match_indices(&needle) {
                let start = ranges[index].0;
                let match_end = ranges[index + needle.len() - 1].1;
                if start < end {
                    continue;
                }
                if start > end {
                    chunks.push((false, &string[end..start]));
                }
                chunks.push((true, &string[start..match_end]));
                end = match_end;
            }
        }
        if end < string.len() {
            chunks.push((false, &string[end..]));
        }
        Self {
            chunks: chunks.into_iter(),
        }
    }
}

impl<'data> Iterator for FindString<'data> {
    type Item = (bool, &'data str);

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next()
    }
}

pub(crate) struct SearchJob {
    pub(crate) job: LayoutJob,
    pub(crate) is_match: bool,
}

pub(crate) fn searchable_text(text: &str, search_string: &str, format: TextFormat) -> SearchJob {
    let mut job = LayoutJob::default();
    let mut is_match = false;
    if !search_string.is_empty() {
        for (m, chunk) in FindString::new(text, search_string) {
            let background = if m {
                is_match = true;
                TextFormat {
                    background: Color32::YELLOW,
                    ..format.clone()
                }
            } else {
                format.clone()
            };
            job.append(chunk, 0.0, background);
        }
    } else {
        job.append(text, 0.0, format);
    }
    SearchJob { job, is_match }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_matches_preserve_original_boundaries_and_text() {
        for (text, query, expected) in [
            ("İ", "i", vec![(true, "İ")]),
            (
                "🙂İxİ",
                "i",
                vec![(false, "🙂"), (true, "İ"), (false, "x"), (true, "İ")],
            ),
            ("İx", "x", vec![(false, "İ"), (true, "x")]),
            ("Árvíz", "ÁR", vec![(true, "Ár"), (false, "víz")]),
            ("Test", "", vec![(false, "Test")]),
            ("", "test", vec![]),
            ("alpha", "z", vec![(false, "alpha")]),
        ] {
            let chunks = FindString::new(text, query).collect::<Vec<_>>();
            assert_eq!(chunks, expected, "{text:?}, {query:?}");
            assert_eq!(
                chunks.iter().map(|(_, part)| *part).collect::<String>(),
                text
            );
        }
    }
}
