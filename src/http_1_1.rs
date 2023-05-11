use bytes::BytesMut;


pub struct HttpRequest<T> {
    head: Header,
    body: T,
}

pub struct Header {
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub headers: HeaderMap<HeaderValue>,
    pub extensions: Extensions,
}

pub enum Method {}
pub enum Version {}
pub struct Uri {

}

pub struct HttpResponse {
}
pub struct HeaderMap<T> {
    values: Vec<T>,
}
pub struct HeaderValue {}
pub struct Extensions {}


impl<T> HttpRequest<T> {
    pub fn parse_request() -> HttpRequest<T> {
        todo!()
    }
}

pub fn create_response() -> HttpResponse {
    todo!()
}

