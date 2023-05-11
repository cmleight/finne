use bytes::BytesMut;


pub struct HttpRequest<T> {
    head: Header,
    body: T,
}

pub struct HttpResponse {
}

pub fn parse_request() -> HttpRequest {
    todo!()
}

pub fn create_response() -> HttpResponse {
    todo!()
}
