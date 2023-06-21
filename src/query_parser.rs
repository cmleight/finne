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

pub struct Term {
    field: String,
    value: String,
}

/*
 * Query syntax:
 *
 * Example query:
 * al:dog and (al:cat or al:mouse) and not al:bird
 */


pub fn parse_query(query: &[u8], query_buffer: &mut [QueryNode]) -> Option<NodeType> {
    let mut offset = 0;
    let mut curr_node = 0;
    while offset < query.len() {
        match query[offset] {
            b'(' => {
                query_buffer[curr_node].node_type = NodeType::Group;
                query_buffer[curr_node].left = Some(curr_node + 1);
                term
            }
        }
    }
    return None;
}
