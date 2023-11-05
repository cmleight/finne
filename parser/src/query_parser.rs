#![allow(dead_code)]

//use nom::IResult;

#[derive(Debug, PartialEq, Eq)]
pub enum NodeType {
    And,
    Or,
    Not,
    Group,
}

pub struct QueryNode {
    node_type: NodeType,
    term: Term,
    left: Option<usize>,
    right: Option<usize>,
}

pub enum TermType {
    Phrase,
    Word,
    Wildcard,
    Fuzzy,
    Proximity,
    Range,
    Boosted,
}

pub struct Term {
    term_type: TermType,
    field: String,
    value: String,
    term_boost: f32,
}

/*
 * Query syntax:
 *
 * Example query:
 * al:dog and (al:cat or al:mouse) and not al:bird
 */

// pub fn parse_query(query: &[u8], query_buffer: &mut [QueryNode]) -> Option<NodeType> {
// }
