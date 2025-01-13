use std::rc::Rc;

pub struct SourceDataItem {
    pub mime_type: Vec<String>,
    pub content: Rc<Vec<u8>>,
}

pub trait SourceData {
    /// Find the best match of the content of the mime_type.
    /// `(result, content)` is returned where the `result` will be false if no content matches
    /// the `mime_type`. In such case content will be an empty vector.
    fn content_by_mime_type(&self, mime_type: &str) -> (bool, Rc<Vec<u8>>);
    /// Returns all supported mime-types.
    fn mime_types(&self) -> Vec<String>;
}

impl SourceData for Vec<SourceDataItem> {
    fn content_by_mime_type(&self, mime_type: &str) -> (bool, Rc<Vec<u8>>) {
        // TODO: Need a more flexible way to match text types.
        log::debug!("content_by_mime_type was called with '{}'", mime_type);
        let mut filter_it = self
            .iter()
            .filter(|item| {
                item.mime_type
                    .iter()
                    .filter(|mt| {
                        log::debug!("check mime-type {mt}");
                        mt.eq_ignore_ascii_case(mime_type)
                    })
                    .peekable()
                    .peek()
                    .is_some()
            })
            .peekable();

        match filter_it.peek() {
            Some(src_data) => (true, src_data.content.clone()),
            _ => {
                log::debug!("The required mime_type '{mime_type}' is not supported");
                (false, Rc::new(vec![]))
            }
        }
    }

    fn mime_types(&self) -> Vec<String> {
        let mut v = Vec::new();
        self.iter().for_each(|item| {
            item.mime_type
                .iter()
                .for_each(|mime_type| v.push(mime_type.clone()));
        });
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::receive_data_bulk;
    use crate::protocol::PROTOCAL_VER;

    #[test]
    fn test_content_by_mime_type() {
        #[rustfmt::skip]
        let buf =
            [0x20, 0x09, 0x02, 0x14, PROTOCAL_VER,
            b'M', 0, 0, 0, 10, b't', b'e', b'x', b't', b'/', b'p', b'l', b'a', b'i', b'n',
            b'M', 0, 0, 0, 4, b'T', b'E', b'X', b'T',
            b'C', 0, 0, 0, 4, b'G', b'O', b'O', b'D',
            b'M', 0, 0, 0, 9, b't', b'e', b'x', b't', b'/', b'h', b't', b'm', b'l',
            b'C', 0, 0, 0, 3, b'B', b'A', b'D',
            ];
        let r = receive_data_bulk(&mut &buf[..]).unwrap();

        let (result, content) = r.content_by_mime_type("text/plain");
        assert!(result);
        assert_eq!(content.as_slice(), b"GOOD");
        let (result, content) = r.content_by_mime_type("text");
        assert!(result);
        assert_eq!(content.as_slice(), b"GOOD");
        let (result, content) = r.content_by_mime_type("TEXT");
        assert!(result);
        assert_eq!(content.as_slice(), b"GOOD");
        let (result, content) = r.content_by_mime_type("text/html");
        assert!(result);
        assert_eq!(content.as_slice(), b"BAD");
        let (result, content) = r.content_by_mime_type("no_mime");
        assert!(!result);
        assert!(content.is_empty());
    }
}
