pub use artifact::table::{
    ColType, Column, ColumnDisplay, ColumnFormat, ColumnLayout, DateFormat, Table,
    rows::{Edit, Page, Revision, Row, Rows},
    store::{Data, Home},
};
mod call;
pub use call::TableCalls;
