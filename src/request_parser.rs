extern crate nom;
use nom::multi::{many_m_n, separated_list0, separated_list1};
use nom::sequence::{preceded, separated_pair, terminated, Tuple};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while},
    character::complete::{alphanumeric1, multispace0, multispace1},
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

#[derive(PartialEq, Debug)]
struct HttpRequest<'a> {
    method: Method,
    path: &'a [u8],
    params: Option<Vec<(&'a [u8], &'a [u8])>>,
    protocol: Protocol,
    headers: Vec<(&'a [u8], Vec<&'a [u8]>)>,
    body: &'a [u8],
}

#[derive(PartialEq, Debug)]
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
fn parse_method(input: &[u8]) -> IResult<&[u8], Method> {
    return terminated(
        alt((
            map(tag_no_case(b"connect"), |_| Method::Connect),
            map(tag_no_case(b"delete"), |_| Method::Delete),
            map(tag_no_case(b"get"), |_| Method::Get),
            map(tag_no_case(b"head"), |_| Method::Head),
            map(tag_no_case(b"options"), |_| Method::Options),
            map(tag_no_case(b"patch"), |_| Method::Patch),
            map(tag_no_case(b"post"), |_| Method::Post),
            map(tag_no_case(b"put"), |_| Method::Put),
            map(tag_no_case(b"trace"), |_| Method::Trace),
        )),
        multispace0,
    )(input);
}

#[inline]
fn get_string(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space(i))(input);
}

#[inline]
fn get_string_non_semicolon(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space(i) && i != b':')(input);
}

#[inline]
fn is_space_or_question(input: u8) -> bool {
    return is_space(input) || input == b'?';
}

#[inline]
fn parse_path(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space_or_question(i))(input);
}

#[inline]
fn parse_params(input: &[u8]) -> IResult<&[u8], Vec<Vec<(&[u8], &[u8])>>> {
    return many_m_n(
        0,
        1,
        preceded(
            tag("?"),
            separated_list0(
                tag("&"),
                separated_pair(alphanumeric1, tag("="), alphanumeric1),
            ),
        ),
    )(input);
}

#[derive(PartialEq, Debug)]
enum Protocol {
    Http10,
    Http11,
}

#[inline]
fn parse_protocol(input: &[u8]) -> IResult<&[u8], Protocol> {
    return alt((
        map(tag_no_case(b"http/1.0"), |_| Protocol::Http10),
        map(tag_no_case(b"http/1.1"), |_| Protocol::Http11),
    ))(input);
}

// TODO: Special case for Set-Cookie since it is permitted to have newlines
#[inline]
fn parse_headers(input: &[u8]) -> IResult<&[u8], Vec<(&[u8], Vec<&[u8]>)>> {
    return terminated(
        separated_list1(
            alt((tag("\r\n"), tag("\n"))),
            separated_pair(
                get_string_non_semicolon,
                |i| (tag(":"), multispace0).parse(i),
                separated_list1(|i| (tag(","), multispace0).parse(i), get_string),
            ),
        ),
        tag("\r\n\r\n"),
    )(input);
}

#[inline]
fn parse_request(req: &[u8]) -> HttpRequest {
    let (body, (method, path, mut params, _, protocol, headers)) = (
        parse_method,
        parse_path,
        parse_params,
        multispace1,
        parse_protocol,
        parse_headers,
    )
        .parse(req)
        .unwrap();
    return HttpRequest {
        method,
        path,
        params: params.pop(),
        protocol,
        headers,
        body,
    };
}

#[cfg(test)]
mod test {
    use super::*;
    use bytes::{BufMut, BytesMut};

    #[test]
    fn test_parse_method() {
        let expected: IResult<&[u8], Method> = Ok((b"", Method::Put));
        assert_eq!(parse_method(b"PUT"), expected);
    }

    #[test]
    fn test_parse_protocol() {
        assert!(parse_protocol(b"HTTP/1.1") == Ok((b"", Protocol::Http11)));
    }

    #[test]
    fn test_parse_path() {
        let res = parse_path(b"/test/ext?one=1&two=2");
        let expected: IResult<&[u8], &[u8]> = Ok((b"?one=1&two=2", b"/test/ext"));
        assert_eq!(res, expected);
    }

    #[test]
    fn test_parse_params() {
        let res = parse_params(b"?one=1&two=2");
        let expected: IResult<&[u8], Vec<Vec<(&[u8], &[u8])>>> =
            Ok((b"", vec![vec![(b"one", b"1"), (b"two", b"2")]]));
        assert_eq!(res, expected);
    }

    #[test]
    fn test_parse_headers() {
        let input = b"Content-Length: length\r\nAccept-Language: en-us, en-gb\r\n";
        let expected: IResult<&[u8], Vec<(&[u8], Vec<&[u8]>)>> = Ok((
            b"",
            vec![
                (b"Content-Length", vec![b"length"]),
                (b"Accept-Language", vec![b"en-us", b"en-gb"]),
            ],
        ));
        assert_eq!(parse_headers(input), expected);
    }

    #[test]
    fn test_parse_request() {
        let example_text: Vec<&[u8]> = vec![
            b"POST /cgi-bin/process.cgi HTTP/1.1\r\n",
            b"User-Agent: Mozilla/4.0 (compatible; MSIE5.01; Windows NT)\r\n",
            b"Host: www.tutorialspoint.com\r\n",
            b"Content-Type: application/x-www-form-urlencoded\r\n",
            b"Content-Length: length\r\n",
            b"Accept-Language: en-us\r\n",
            b"Accept-Encoding: gzip, deflate\r\n",
            b"Connection: Keep-Alive",
            b"\r\n\r\n",
            b"licenseID=string&content=string&/paramsXML=string",
        ];
        let mut test_arr = BytesMut::new();
        for arr in example_text {
            test_arr.put_slice(arr);
        }
        let res = parse_request(&test_arr);
        assert_eq!(
            res,
            HttpRequest {
                method: Method::Connect,
                path: &[],
                params: None,
                protocol: Protocol::Http10,
                headers: vec![],
                body: &[],
            }
        );
    }
}
