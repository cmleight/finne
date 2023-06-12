extern crate nom;

use nom::{
    branch::alt,
    bytes::complete::{tag_no_case},
    character::complete::multispace0,
    combinator::map,
    IResult,
    error::VerboseError,
    InputTakeAtPosition,
    AsChar
};

// METHOD PATH PROTOCOL
// GENERAL-HEADER - (Connection, Date)
// REQUEST-HEADER - (HashMap)
// ENTITY-HEADER - (Content-Length, Content-Type, Last-Modified)
// \r\n\r\n
// BODY

struct HttpRequest {
    method: Method,
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

fn consume_spaces(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return multispace0(input);
}

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

fn get_path(input: &[u8]) -> IResult<&[u8], &[u8]> {

}

// fn parse_request<'a>(req: Bytes) -> IResult<&'a Bytes, HttpRequest> {}

mod test {
    use bytes::Bytes;
    use super::*;

    #[test]
    fn test_get_method() {
        assert!(get_method(b"PUT") == Ok((b"", Method::Put)));
    }
}