use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, parse_macro_input};

fn byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || byte == ('-' as u8)
        || byte == ('.' as u8)
        || byte == ('_' as u8)
        || byte == ('~' as u8)
}

enum ParsePathError {
    ComponentTooLong(usize),
    FancyCharacter(char),
    InvalidPercentEncoding,
    PathTooLong(usize),
    TooManyComponents(usize),
}

// The successful return consists of the component bytes, and number of parsed input bytes. If the number of input bytes is less than `s.len()`, then the component was terminated by a slash.
fn parse_component(
    s: &str,
    max_component_len: usize,
) -> core::result::Result<(usize, Vec<u8>), ParsePathError> {
    let mut comp_data = vec![];

    let mut percent_state = 0; // 0 if not parsing a percent encoding, 1 when parsing its first character, 2 when parsing its second character. This is hacky but I don't care =S
    let mut high_nibble = 0u8;

    for (offset, c) in s.char_indices() {
        if percent_state == 0 {
            if c == '/' {
                if comp_data.len() > max_component_len {
                    return Err(ParsePathError::ComponentTooLong(comp_data.len()));
                } else {
                    return Ok((offset + c.len_utf8(), comp_data));
                }
            } else if c.is_ascii() {
                let mut buf = [0];
                c.encode_utf8(&mut buf);

                if byte_is_unreserved(buf[0]) {
                    comp_data.push(buf[0]);
                } else if c == '%' {
                    percent_state += 1;
                } else {
                    return Err(ParsePathError::FancyCharacter(c));
                }
            } else {
                return Err(ParsePathError::FancyCharacter(c));
            }
        } else if percent_state == 1 {
            if c.is_ascii_hexdigit() {
                high_nibble = (c.to_digit(16).unwrap() as u8) << 4;
                percent_state = 2;
            } else {
                return Err(ParsePathError::InvalidPercentEncoding);
            }
        } else {
            debug_assert!(percent_state == 2);

            if c.is_ascii_hexdigit() {
                let new_byte = high_nibble + (c.to_digit(16).unwrap() as u8);
                comp_data.push(new_byte);
                percent_state = 0;
            } else {
                return Err(ParsePathError::InvalidPercentEncoding);
            }
        }
    }

    if percent_state == 0 {
        if comp_data.len() > max_component_len {
            return Err(ParsePathError::ComponentTooLong(comp_data.len()));
        } else {
            return Ok((s.len(), comp_data));
        }
    } else {
        return Err(ParsePathError::InvalidPercentEncoding);
    }
}

fn parse_path(
    s: &str,
    max_component_len: usize,
    max_component_count: usize,
    max_path_len: usize,
) -> core::result::Result<Vec<Vec<u8>>, ParsePathError> {
    let input_len = s.len();
    let mut offset = 1; // Skip past the leading slash.

    let mut comps = vec![];
    let mut decoded_len = 0;

    while offset < input_len {
        let (parsed, comp) = parse_component(
            unsafe { core::str::from_utf8_unchecked(&s.as_bytes()[offset..]) },
            max_component_len,
        )?;
        offset += parsed;
        decoded_len += comp.len();
        comps.push(comp);
    }

    // Due to the way the parser is implemented, a trailing slash is swallowed, so we explicity add an empty component here if the literal ends with a slash.
    if s.as_bytes()[input_len - 1] == 0x2f {
        comps.push(vec![]);
    }

    if decoded_len > max_path_len {
        return Err(ParsePathError::PathTooLong(decoded_len));
    } else if comps.len() > max_component_count {
        return Err(ParsePathError::TooManyComponents(comps.len()));
    } else {
        return Ok(comps);
    }
}

#[proc_macro]
pub fn component_internal(input: TokenStream) -> TokenStream {
    let string_lit = parse_macro_input!(input as LitStr);
    let content = string_lit.value();

    let expanded = match parse_component(&content, 4096) {
        Err(ParsePathError::ComponentTooLong(_actual_len)) => syn::Error::new(
                string_lit.span(),
                "A component must not consist of more than 4096 (decoded) bytes.",
            )
            .to_compile_error(),
        Err(ParsePathError::FancyCharacter(_char)) => syn::Error::new(
                string_lit.span(),
                "Components must be specified using only ascii alphanumerics, one of the - . _ ~ characters, or a percent encoding.",
            )
            .to_compile_error(),
        Err(ParsePathError::InvalidPercentEncoding) => syn::Error::new(
                string_lit.span(),
                "Percent encodings must consist of a % character followed by exactly two ascii hex digits.",
            )
            .to_compile_error(),
        Err(ParsePathError::PathTooLong(_len)) | Err(ParsePathError::TooManyComponents(_len)) => unreachable!(),
        Ok((len, comp_content)) => {
            if len < content.len() {
                syn::Error::new(
                    string_lit.span(),
                    "Individual path components cannot contain the / character (use a percent encoding if the component should indeed contain the byte 0x2F, i.e., an ascii forward slash)",
                )
                .to_compile_error()
            } else {
                quote! {
                    &[ #(#comp_content),* ].as_slice()
                }
            }
        }
    };

    return TokenStream::from(expanded);
}

#[proc_macro]
pub fn path_internal(input: TokenStream) -> TokenStream {
    let string_lit = parse_macro_input!(input as LitStr);
    let content = string_lit.value();

    if content == "" {
        return quote! {
            &[]
        }
        .into();
    } else if content.as_bytes()[0] != 0x2f
    /* `/` */
    {
        return syn::Error::new(
            string_lit.span(),
            "Every non-empty path literal must start with a forward slash.",
        )
        .to_compile_error()
        .into();
    }

    let expanded = match parse_path(&content, 4096, 4096, 4096) {
        Err(ParsePathError::ComponentTooLong(_actual_len)) => syn::Error::new(
                string_lit.span(),
                "Any individual path component must not consist of more than 4096 (decoded) bytes.",
            )
        .to_compile_error(),
        Err(ParsePathError::FancyCharacter(_char)) => syn::Error::new(
                string_lit.span(),
                "Path components must be specified using only ascii alphanumerics, one of the - . _ ~ characters, or a percent encoding.",
            )
            .to_compile_error(),
        Err(ParsePathError::InvalidPercentEncoding) => syn::Error::new(
                string_lit.span(),
                "Percent encodings must consist of a % character followed by exactly two ascii hex digits.",
            )
            .to_compile_error(),
        Err(ParsePathError::PathTooLong(_len)) => syn::Error::new(
                string_lit.span(),
                "A path must not consist of more than 4096 (decoded) bytes.",
            )
            .to_compile_error(),
        Err(ParsePathError::TooManyComponents(_count)) => syn::Error::new(
                string_lit.span(),
                "A path must not consist of more than 4096 individual components.",
            )
            .to_compile_error(),
        Ok(path_contents) => {
            let mut comps: Vec<proc_macro2::TokenStream> = vec![];

            for comp in path_contents {
                comps.push(quote! {
                    &[ #(#comp),* ]
                });
            }

            quote! {
                &[ #(#comps),* ]
            }
        }
    };

    return TokenStream::from(expanded);
}
