use crate::ir::{Expression, Value, PropertyKey};

pub fn get_function_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Value(Value::Variable(name)) => Some(name.clone()),
        Expression::Member { property: PropertyKey::Ident(name), .. } => Some(name.clone()),
        _ => None,
    }
}

pub fn name_for_call(func_name: &str) -> String {
    match func_name.to_lowercase().as_str() {
        "fetch" => "response".to_string(),
        "json" => "data".to_string(),
        "parse" | "parseint" | "parsefloat" => "parsed".to_string(),
        "stringify" => "json".to_string(),
        "getitem" | "get" => "value".to_string(),
        "setitem" | "set" => "result".to_string(),
        "find" | "filter" => "found".to_string(),
        "map" => "mapped".to_string(),
        "reduce" => "reduced".to_string(),
        "split" => "parts".to_string(),
        "join" => "joined".to_string(),
        "slice" | "substring" | "substr" => "substr".to_string(),
        "tostring" => "str".to_string(),
        "tolowercase" | "touppercase" => "formatted".to_string(),
        "trim" => "trimmed".to_string(),
        "replace" | "replaceall" => "replaced".to_string(),
        "match" | "exec" => "match".to_string(),
        "test" => "isMatch".to_string(),
        "keys" => "keys".to_string(),
        "values" => "values".to_string(),
        "entries" => "entries".to_string(),
        "assign" | "create" => "obj".to_string(),
        "concat" => "combined".to_string(),
        "push" | "pop" | "shift" | "unshift" => "arr".to_string(),
        "sort" => "sorted".to_string(),
        "reverse" => "reversed".to_string(),
        "includes" | "has" | "contains" => "hasItem".to_string(),
        "indexof" => "index".to_string(),
        "foreach" => "item".to_string(),
        "settimeout" | "setinterval" => "timerId".to_string(),
        "mutate" => "mutation".to_string(), // Explicit Apollo pattern
        "query" => "query".to_string(),
        "subscribe" => "subscription".to_string(),
        "promise" => "promiseObj".to_string(),
        "then" => "nextPromise".to_string(),
        "catch" => "catchPromise".to_string(),
        "finally" => "cleanupPromise".to_string(),
        "require" => "module".to_string(),
        "createelement" => "element".to_string(),
        "getelementbyid" | "queryselector" => "element".to_string(),
        "queryselectorall" | "getelementsbytagname" | "getelementsbyclassname" => "elements".to_string(),
        "addeventlistener" => "listener".to_string(),
        "removeeventlistener" => "removed".to_string(),
        "classlist" => "classes".to_string(),
        "style" => "style".to_string(),
        "getattribute" | "setattribute" => "attr".to_string(),
        _ => {
            // Use function name as base if it's reasonable
            if func_name.len() <= 20 && func_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                format!("{}Result", func_name)
            } else {
                "result".to_string()
            }
        }
    }
}

pub fn name_for_property(prop: &str) -> String {
    match prop.to_lowercase().as_str() {
        "length" => "len".to_string(),
        "prototype" => "proto".to_string(),
        "constructor" => "ctor".to_string(),
        "name" => "name".to_string(),
        "value" => "val".to_string(),
        "data" => "data".to_string(),
        "result" | "results" => "result".to_string(),
        "error" | "errors" => "err".to_string(),
        "message" | "msg" => "msg".to_string(),
        "status" => "status".to_string(),
        "code" => "code".to_string(),
        "type" => "type".to_string(),
        "id" => "id".to_string(),
        "key" => "key".to_string(),
        "index" | "idx" => "idx".to_string(),
        "item" | "items" => "item".to_string(),
        "element" | "elements" | "elem" => "elem".to_string(),
        "node" | "nodes" => "node".to_string(),
        "child" | "children" => "child".to_string(),
        "parent" => "parent".to_string(),
        "next" | "prev" | "previous" => prop.to_string(),
        "first" | "last" => prop.to_string(),
        "start" | "end" | "begin" => prop.to_string(),
        "count" | "total" | "size" => "count".to_string(),
        "width" | "height" => prop.to_string(),
        "x" | "y" | "z" => prop.to_string(),
        "left" | "right" | "top" | "bottom" => prop.to_string(),
        "config" | "configuration" | "settings" | "options" => "config".to_string(),
        "state" => "state".to_string(),
        "props" | "properties" => "props".to_string(),
        "context" | "ctx" => "ctx".to_string(),
        "callback" | "cb" => "callback".to_string(),
        "handler" => "handler".to_string(),
        "listener" => "listener".to_string(),
        "event" | "evt" | "e" => "event".to_string(),
        "request" | "req" => "req".to_string(),
        "response" | "res" => "res".to_string(),
        "body" => "body".to_string(),
        "headers" => "headers".to_string(),
        "url" | "uri" | "href" => "url".to_string(),
        "path" | "pathname" => "path".to_string(),
        "query" | "search" => "query".to_string(),
        "params" | "parameters" => "params".to_string(),
        "args" | "arguments" => "args".to_string(),
        _ => {
            // Use property name if reasonable
            if prop.len() <= 15 && prop.chars().all(|c| c.is_alphanumeric() || c == '_') {
                sanitize_name(prop)
            } else {
                "prop".to_string()
            }
        }
    }
}

pub fn name_for_instance(class_name: &str) -> String {
    // Convert PascalCase to camelCase
    let mut chars = class_name.chars();
    match chars.next() {
        Some(first) => {
            let lower_first = first.to_lowercase().to_string();
            let rest: String = chars.collect();
            let base = format!("{}{}", lower_first, rest);
            sanitize_name(&base)
        }
        None => "instance".to_string(),
    }
}

pub fn sanitize_name(name: &str) -> String {
    // Remove invalid characters and ensure valid JS identifier
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if cleaned.is_empty() {
        return "var".to_string();
    }

    // Ensure doesn't start with a digit
    if cleaned.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return format!("v{}", cleaned);
    }

    // Check for reserved words
    if is_reserved_word(&cleaned) {
        return format!("_{}", cleaned);
    }

    cleaned
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "break" | "case" | "catch" | "continue" | "debugger" | "default" | "delete"
            | "do" | "else" | "finally" | "for" | "function" | "if" | "in" | "instanceof"
            | "new" | "return" | "switch" | "this" | "throw" | "try" | "typeof" | "var"
            | "void" | "while" | "with" | "class" | "const" | "enum" | "export" | "extends"
            | "import" | "super" | "implements" | "interface" | "let" | "package" | "private"
            | "protected" | "public" | "static" | "yield" | "null" | "true" | "false"
            | "undefined" | "NaN" | "Infinity" | "arguments" | "eval"
    )
}
