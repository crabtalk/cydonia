mod common;

use common::{Scratch, refused, said};
use cydonia_mcp::{Server, tool::Answer, tools::table};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

fn call(server: &Server, scratch: &Scratch, name: &str, args: Value) -> Answer {
    server
        .call(name, args, Some(scratch.path()))
        .unwrap_or_else(|_| panic!("{name} failed"))
}

#[test]
fn agents_create_edit_query_and_delete_tables() {
    let scratch = Scratch::new("table-lifecycle");
    let server = scratch.server();
    let created = call(
        &server,
        &scratch,
        "table_create",
        json!({
            "name":"Tasks", "key":"tasks", "columns":[{"name":"Title","type":"text"}]
        }),
    )
    .data
    .unwrap();
    let reference = format!("#{}", created["number"]);
    let inserted = call(
        &server,
        &scratch,
        "table_insert",
        json!({
            "table":reference,"rows":[{"Title":"Draft"},{"Title":"Review"}]
        }),
    )
    .data
    .unwrap();
    let ids = inserted["rowids"].as_array().unwrap();
    call(
        &server,
        &scratch,
        "table_write_column",
        json!({"table":reference,"column":"Done","type":"check"}),
    );
    call(
        &server,
        &scratch,
        "table_write_cells",
        json!({
            "table":reference,"edits":[{"rowid":ids[0],"column":"Done","value":true}]
        }),
    );
    let page = call(
        &server,
        &scratch,
        "table_read",
        json!({"table":reference,"limit":1,"offset":1}),
    )
    .data
    .unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["rows"][0]["rowid"], ids[1]);
    let rows = call(
        &server,
        &scratch,
        "table_query",
        json!({"sql":"SELECT Title FROM tasks WHERE Done = 1;"}),
    )
    .data
    .unwrap();
    assert_eq!(rows["rows"], json!([["Draft"]]));
    call(
        &server,
        &scratch,
        "table_rename",
        json!({"table":reference,"name":"Work"}),
    );
    assert_eq!(
        call(&server, &scratch, "table_read", json!({"table":"Work"}))
            .data
            .unwrap()["key"],
        "tasks"
    );
    call(
        &server,
        &scratch,
        "table_delete_rows",
        json!({"table":reference,"rowids":[ids[0]]}),
    );
    call(
        &server,
        &scratch,
        "table_write_column",
        json!({"table":reference,"column":"Title","type":"number","rename":"Name"}),
    );
    let page = call(&server, &scratch, "table_read", json!({"table":reference}))
        .data
        .unwrap();
    assert_eq!(page["rows"][0]["rowid"], ids[1]);
    call(
        &server,
        &scratch,
        "table_delete_column",
        json!({"table":reference,"column":"Done"}),
    );
    let listed = call(&server, &scratch, "table_list", json!({}))
        .data
        .unwrap();
    assert_eq!(listed[0]["rows"], 1);
    assert_eq!(listed[0]["columns"].as_array().unwrap().len(), 1);
    call(
        &server,
        &scratch,
        "table_delete",
        json!({"table":reference}),
    );
    assert_eq!(
        call(&server, &scratch, "table_list", json!({}))
            .data
            .unwrap(),
        json!([])
    );
}

#[test]
fn table_tools_respect_write_and_delete_switches() {
    let scratch = Scratch::new("table-permissions");
    let server = Server::new()
        .mount(&table::TOOLS)
        .writable(Arc::new(AtomicBool::new(false)));
    let request =
        serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).unwrap();
    let listed = server
        .handle(&request, Some(scratch.path()))
        .unwrap()
        .result
        .unwrap();
    let offered: Vec<_> = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(offered, ["table_list", "table_read", "table_query"]);
    assert!(
        server
            .call("table_create", json!({}), Some(scratch.path()))
            .is_err()
    );
    let server = Server::new().mount(&table::TOOLS);
    assert!(
        server
            .call("table_delete", json!({}), Some(scratch.path()))
            .is_err()
    );
}

#[test]
fn table_queries_cannot_write_and_internal_tables_are_not_editable() {
    let scratch = Scratch::new("table-query-boundaries");
    let server = scratch.server();
    call(
        &server,
        &scratch,
        "table_create",
        json!({"name":"Items","columns":[{"name":"Value","type":"text"}]}),
    );
    assert!(
        !refused(server.call(
            "table_query",
            json!({"sql":"DELETE FROM items"}),
            Some(scratch.path())
        ))
        .is_empty()
    );
    assert!(
        !refused(server.call(
            "table_insert",
            json!({"table":"_tables","rows":[{}]}),
            Some(scratch.path())
        ))
        .is_empty()
    );
    let rows = call(&server, &scratch, "table_query", json!({"sql":"WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1200) SELECT x FROM n"})).data.unwrap();
    assert_eq!(rows["rows"].as_array().unwrap().len(), 1000);
}

#[test]
fn a_table_reference_can_name_another_open_project() {
    let first = Scratch::new("table-first");
    let second = Scratch::new("table-second");
    let server = first.server();
    let _ = second.server();
    let added = call(
        &server,
        &second,
        "table_create",
        json!({"name":"Remote","columns":[{"name":"Value","type":"number"}]}),
    )
    .data
    .unwrap();
    let project = second.path().file_name().unwrap().to_str().unwrap();
    let reference = format!("{project}#{}", added["number"]);
    let page = call(&server, &first, "table_read", json!({"table":reference}))
        .data
        .unwrap();
    assert_eq!(page["name"], "Remote");
    assert!(
        said(server.call("project_entries", json!({}), Some(second.path()))).contains("Remote")
    );
}
