extern crate nom;

use bytes::BytesMut;
use nom::multi::{many_m_n, separated_list0, separated_list1};
use nom::sequence::{preceded, separated_pair, terminated, Tuple};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while},
    character::complete::{alphanumeric1, multispace0},
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
pub struct HttpRequest<'a> {
    pub method: Method,
    pub path: &'a [u8],
    params: Option<Vec<(&'a [u8], &'a [u8])>>,
    pub protocol: Protocol,
    pub headers: Vec<(&'a [u8], Vec<&'a [u8]>)>,
    pub body: &'a [u8],
}

impl HttpRequest<'_> {
    pub fn get_parameter(&self, key: &str) -> Option<&[u8]> {
        return match self.params {
            Some(ref params) => params
                .iter()
                .find(|&(k, _)| k == &key.as_bytes())
                .map(|&(_, v)| v)
                .take(),
            None => None,
        };
    }
}

#[derive(PartialEq, Debug)]
pub enum Method {
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
fn get_string_param(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space(i) && i != b'&')(input);
}

#[inline]
fn get_string_non_semicolon(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| !is_space(i) && i != b':')(input);
}

#[inline]
fn get_string_non_comma(input: &[u8]) -> IResult<&[u8], &[u8]> {
    return take_while(|i| match i {
        b',' | b'\r' | b'\n' => false,
        _ => true,
    })(input);
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
                separated_pair(alphanumeric1, tag("="), get_string_param),
            ),
        ),
    )(input);
}

#[derive(PartialEq, Debug)]
pub enum Protocol {
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
    return separated_list1(
        alt((tag("\r\n"), tag("\n"))),
        preceded(
            multispace0,
            separated_pair(get_string_non_semicolon, tag(":"), parse_header_value),
        ),
    )(input);
}

#[inline]
fn parse_header_value(input: &[u8]) -> IResult<&[u8], Vec<&[u8]>> {
    return separated_list1(tag(","), preceded(multispace0, get_string_non_comma))(input);
}

#[inline]
pub fn parse_request(req: &[u8]) -> Result<HttpRequest, nom::Err<nom::error::Error<&[u8]>>> {
    return match (
        parse_method,
        parse_path,
        parse_params,
        multispace0,
        parse_protocol,
        parse_headers,
        preceded(tag("\r\n\r\n"), multispace0),
    )
        .parse(req)
    {
        Ok((body, (method, path, mut params, _, protocol, headers, _))) => Ok(HttpRequest {
            method,
            path,
            params: params.pop(),
            protocol,
            headers,
            body,
        }),
        Err(a) => Err(a),
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
        let input = b"Content-Length: length\r\nAccept-Language: en-us, en-gb";
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
    fn test_parse_header_value() {
        let input = b" en-us, en-gb";
        let expected: IResult<&[u8], Vec<&[u8]>> = Ok((b"", vec![b"en-us", b"en-gb"]));
        assert_eq!(parse_header_value(input), expected)
    }

    #[test]
    fn test_parse_request() {
        let example_text: Vec<&[u8]> = vec![
            b"POST /cgi-bin/process.cgi?test=1&two=2 HTTP/1.1\r\n",
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
        let expected_params: Option<Vec<(&[u8], &[u8])>> =
            Some(vec![(b"test", b"1"), (b"two", b"2")]);
        assert_eq!(
            res,
            Ok(HttpRequest {
                method: Method::Post,
                path: b"/cgi-bin/process.cgi",
                params: expected_params,
                protocol: Protocol::Http11,
                headers: vec![
                    (
                        b"User-Agent",
                        vec![b"Mozilla/4.0 (compatible; MSIE5.01; Windows NT)"]
                    ),
                    (b"Host", vec![b"www.tutorialspoint.com"]),
                    (b"Content-Type", vec![b"application/x-www-form-urlencoded"]),
                    (b"Content-Length", vec![b"length"]),
                    (b"Accept-Language", vec![b"en-us"]),
                    (b"Accept-Encoding", vec![b"gzip", b"deflate"]),
                    (b"Connection", vec![b"Keep-Alive"]),
                ],
                body: b"licenseID=string&content=string&/paramsXML=string",
            })
        );
    }

    #[test]
    fn test_parse_firefox_request() {
        let example_text: Vec<&[u8]> = vec![
            b"GET /s?q=su:dog HTTP/1.1\r\n",
            b"Host: localhost:3000\r\n",
            b"User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/114.0\r\n",
            b"Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8\r\n",
            b"Accept-Language: en-US,en;q=0.5\r\n",
            b"Accept-Encoding: gzip, deflate, br\r\n",
            b"Connection: keep-alive\r\n",
            b"Cookie: Clion-bcf0c5d2=c123866b-c839-4af5-bd38-5fa78bcfea64\r\n",
            b"Upgrade-Insecure-Requests: 1\r\n",
            b"Sec-Fetch-Dest: document\r\n",
            b"Sec-Fetch-Mode: navigate\r\n",
            b"Sec-Fetch-Site: none\r\n",
            b"Sec-Fetch-User: ?1\r\n\r\n",
        ];
        let mut test_arr = BytesMut::new();
        for arr in example_text {
            test_arr.put_slice(arr);
        }
        let res = parse_request(&test_arr);
        let params: Option<Vec<(&[u8], &[u8])>> = Some(vec![(b"q", b"su:dog")]);
        assert_eq!(
            res,
            Ok(HttpRequest {
                method: Method::Get,
                path: b"/s",
                params,
                protocol: Protocol::Http11,
                headers: vec![
                    (b"Host", vec![b"localhost:3000"]),
                    (
                        b"User-Agent",
                        vec![b"Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/114.0"]
                    ),
                    (
                        b"Accept",
                        vec![b"text/html", b"application/xhtml+xml", b"application/xml;q=0.9", b"image/avif", b"image/webp", b"*/*;q=0.8"]
                    ),
                    (b"Accept-Language", vec![b"en-US", b"en;q=0.5"]),
                    (b"Accept-Encoding", vec![b"gzip", b"deflate", b"br"]),
                    (b"Connection", vec![b"keep-alive"]),
                    (b"Cookie", vec![b"Clion-bcf0c5d2=c123866b-c839-4af5-bd38-5fa78bcfea64"]),
                    (b"Upgrade-Insecure-Requests", vec![b"1"]),
                    (b"Sec-Fetch-Dest", vec![b"document"]),
                    (b"Sec-Fetch-Mode", vec![b"navigate"]),
                    (b"Sec-Fetch-Site", vec![b"none"]),
                    (b"Sec-Fetch-User", vec![b"?1"]),
                ],
                body: b"",
            })
        );
    }
}
