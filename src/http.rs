use bytes::BytesMut;


pub struct HttpRequest<T> {
    head: Header,
    body: T,
}

pub struct Header {
}

pub struct HttpResponse {
}


impl<T> HttpRequest<T> {
    pub fn parse_request() -> HttpRequest<T> {
        todo!()
    }
}

pub fn create_response() -> HttpResponse {
    todo!()
}
