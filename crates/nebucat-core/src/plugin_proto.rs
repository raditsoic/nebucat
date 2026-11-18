use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub keyword: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObj>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorObj {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    OpenUrl { url: String },
    OpenPath { path: String },
    Copy { text: String },
    Run { command: String },
    Plugin { data: Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    #[serde(default)]
    pub hide: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

pub fn query_request(id: u64, query: &str) -> String {
    json!({ "id": id, "method": "query", "params": { "query": query } }).to_string()
}

pub fn action_request(id: u64, data: &Value) -> String {
    json!({ "id": id, "method": "action", "params": { "data": data } }).to_string()
}

pub fn parse_items(result: &Value) -> Result<Vec<Item>, String> {
    let items = result
        .get("items")
        .ok_or_else(|| "result has no \"items\" field".to_string())?;
    Vec::<Item>::deserialize(items).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_from_toml() {
        let m: Manifest = toml::from_str(
            "id = \"hello\"\nname = \"Hello\"\nkeyword = \"hi\"\ncommand = [\"python\", \"main.py\"]\n",
        )
        .unwrap();
        assert_eq!(
            m,
            Manifest {
                id: "hello".into(),
                name: "Hello".into(),
                keyword: "hi".into(),
                command: vec!["python".into(), "main.py".into()],
            }
        );
    }

    #[test]
    fn manifest_rejects_unknown_key() {
        let r: Result<Manifest, _> = toml::from_str(
            "id = \"a\"\nname = \"A\"\nkeyword = \"a\"\ncommand = [\"x\"]\nextra = 1\n",
        );
        assert!(r.is_err());
    }

    #[test]
    fn action_variants_roundtrip() {
        let cases = [
            (json!({"type":"open_url","url":"x"}), Action::OpenUrl { url: "x".into() }),
            (json!({"type":"open_path","path":"C:\\a"}), Action::OpenPath { path: "C:\\a".into() }),
            (json!({"type":"copy","text":"t"}), Action::Copy { text: "t".into() }),
            (json!({"type":"run","command":"echo hi"}), Action::Run { command: "echo hi".into() }),
            (json!({"type":"plugin","data":{"text":"q"}}), Action::Plugin { data: json!({"text":"q"}) }),
        ];
        for (wire, action) in cases {
            let item_json = json!({ "title": "T", "action": wire });
            let item: Item = serde_json::from_value(item_json.clone()).unwrap();
            assert_eq!(item.action.as_ref(), Some(&action));
            assert_eq!(serde_json::to_value(&item).unwrap(), item_json);
        }
    }

    #[test]
    fn item_optional_fields() {
        let item: Item = serde_json::from_str(
            r#"{"title":"T","subtitle":"s","icon":"i.png","highlight":[0,2]}"#,
        )
        .unwrap();
        assert_eq!(item.subtitle.as_deref(), Some("s"));
        assert_eq!(item.icon.as_deref(), Some("i.png"));
        assert_eq!(item.highlight, Some(vec![0, 2]));
        assert_eq!(item.action, None);
        let bare: Item = serde_json::from_str(r#"{"title":"T"}"#).unwrap();
        assert_eq!(serde_json::to_string(&bare).unwrap(), r#"{"title":"T"}"#);
    }

    #[test]
    fn response_with_error() {
        let r: Response = serde_json::from_str(r#"{"id":3,"error":{"message":"boom"}}"#).unwrap();
        assert_eq!(r.id, 3);
        assert_eq!(r.result, None);
        assert_eq!(r.error, Some(ErrorObj { message: "boom".into() }));
    }

    #[test]
    fn response_with_result() {
        let r: Response =
            serde_json::from_str(r#"{"id":1,"result":{"items":[{"title":"a"}]}}"#).unwrap();
        let items = parse_items(r.result.as_ref().unwrap()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "a");
    }

    #[test]
    fn parse_items_errors() {
        assert!(parse_items(&json!({})).is_err());
        assert!(parse_items(&json!({"items":[{"subtitle":"no title"}]})).is_err());
    }

    #[test]
    fn action_result_defaults() {
        let r: ActionResult = serde_json::from_str("{}").unwrap();
        assert_eq!(r, ActionResult { hide: false, query: None });
        let r: ActionResult = serde_json::from_str(r#"{"hide":false,"query":"hi CAT"}"#).unwrap();
        assert_eq!(r.query.as_deref(), Some("hi CAT"));
    }

    #[test]
    fn requests_are_single_json_lines() {
        let q = query_request(7, "a\nb");
        assert!(!q.contains('\n'));
        let v: Value = serde_json::from_str(&q).unwrap();
        assert_eq!(v, json!({"id":7,"method":"query","params":{"query":"a\nb"}}));
        let req: Request = serde_json::from_str(&q).unwrap();
        assert_eq!(req.method, "query");

        let a = action_request(8, &json!({"text":"cat"}));
        assert!(!a.contains('\n'));
        let v: Value = serde_json::from_str(&a).unwrap();
        assert_eq!(v, json!({"id":8,"method":"action","params":{"data":{"text":"cat"}}}));
    }
}
