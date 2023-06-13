extern crate nom;
use std::collections::HashMap;
use nom::{
    branch::alt,
    bytes::complete::{tag_no_case, take_until, take_while},
    character::complete::multispace0,
    character::is_space,
    combinator::map,
    IResult,
};

// METHOD PATH PROTOCOL
// GENERAL-HEADER - (Connection, Date)
// REQUEST-HEADER - (HashMap)
// ENTITY-HEADER - (Content-Length, Content-Type, Last-Modified)
// \r\n\r\n
// BODY

struct HttpRequest<'a> {
    method: Method,
    path: String,
    params: HashMap<&'a str, &'a Vec<&'a str>>,
    protocol: Protocol,
}

#[derive(PartialEq)]
enum Method {
    Connect,
    Delete,
    Get,
    Head,
    Options,
    Patch,
    Post,
    Put,
    Trace,
}

#[inline]
fn consume_spaces(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return multispace0(input);
}

#[inline]
fn get_method(input: &[u8]) -> IResult<&[u8], Method> {
    return alt((
        map(tag_no_case(b"connect"), |_| Method::Connect),
        map(tag_no_case(b"delete"), |_| Method::Delete),
        map(tag_no_case(b"get"), |_| Method::Get),
        map(tag_no_case(b"head"), |_| Method::Head),
        map(tag_no_case(b"options"), |_| Method::Options),
        map(tag_no_case(b"patch"), |_| Method::Patch),
        map(tag_no_case(b"post"), |_| Method::Post),
        map(tag_no_case(b"put"), |_| Method::Put),
        map(tag_no_case(b"trace"), |_| Method::Trace),
    ))(input);
}

#[inline]
fn get_string(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space(i))(input);
}

#[inline]
fn is_space_or_question(input: u8) -> bool {
    return is_space(input) || input == b'?';
}

#[inline]
fn get_path(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space_or_question(i))(input);
}

// fn get_params(input: &[u8]) -> IResult<&[u8], HashMap<&[u8], &[u8]>> {
//     return take_until()
// }

#[derive(PartialEq)]
enum Protocol {
    Http10,
    Http11,
}

fn parse_protocol(input: &[u8]) -> IResult<&[u8], Protocol> {
    return alt((
        map(tag_no_case(b"http/1.0"), |_| Protocol::Http10),
        map(tag_no_case(b"http/1.1"), |_| Protocol::Http11),
    ))(input);
}

// fn parse_headers<'a>(input: &[u8], headers: HashMap<&[u8], &[u8]>) -> IResult<&'a [u8], HashMap<&'a [u8], &'a [u8]>> {
//     take_until()
// }

// fn parse_request<'a>(req: Bytes) -> IResult<&'a Bytes, HttpRequest> {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_method() {
        assert!(get_method(b"PUT") == Ok((b"", Method::Put)));
    }
}

