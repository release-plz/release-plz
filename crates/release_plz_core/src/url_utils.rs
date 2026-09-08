use std::borrow::Cow;

/// Percent-encode a URL component, preserving only ASCII alphanumerics and `-._~`.
pub(crate) fn encode_component(value: &str) -> Cow<'_, str> {
    fn is_unreserved(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
    }

    if value.bytes().all(is_unreserved) {
        return Cow::Borrowed(value);
    }

    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if is_unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    Cow::Owned(encoded)
}

#[cfg(test)]
mod tests {
    use super::encode_component;

    #[test]
    fn url_components_are_percent_encoded() {
        for (input, expected) in [
            ("", ""),
            ("azAZ09-._~", "azAZ09-._~"),
            ("group/subgroup/repo", "group%2Fsubgroup%2Frepo"),
            ("release/v1.0+test%2F", "release%2Fv1.0%2Btest%252F"),
            ("a b?c=d&e#f!*'()", "a%20b%3Fc%3Dd%26e%23f%21%2A%27%28%29"),
            ("café/🚀", "caf%C3%A9%2F%F0%9F%9A%80"),
            ("\0\t\n\r\u{7f}", "%00%09%0A%0D%7F"),
        ] {
            assert_eq!(encode_component(input), expected, "input: {input:?}");
        }
    }
}
