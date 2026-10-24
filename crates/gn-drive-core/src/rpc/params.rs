//! Request parameter accessor (port of `RPCParams`).

use super::error::RpcError;
use super::json::{bool_value, int_value, string_of};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default)]
pub struct Params(pub Map<String, Value>);

impl Params {
    /// `null` → empty; object → itself; anything else → `invalid_params`.
    pub fn new(value: &Value) -> Result<Params, RpcError> {
        match value {
            Value::Null => Ok(Params(Map::new())),
            Value::Object(m) => Ok(Params(m.clone())),
            _ => Err(RpcError::new("invalid_params", "params must be an object")),
        }
    }

    pub fn from_map(m: Map<String, Value>) -> Params {
        Params(m)
    }

    fn aliases(key: &str) -> Vec<String> {
        let mut out = vec![key.to_string()];
        if let Some(s) = key.strip_suffix("ID") {
            out.push(format!("{s}Id"));
        }
        if let Some(s) = key.strip_suffix("JSON") {
            out.push(format!("{s}Json"));
        }
        if let Some(s) = key.strip_suffix("IDs") {
            out.push(format!("{s}Ids"));
        }
        out
    }

    pub fn any(&self, key: &str) -> Option<&Value> {
        for k in Self::aliases(key) {
            if let Some(v) = self.0.get(&k)
                && !v.is_null()
            {
                return Some(v);
            }
        }
        None
    }

    pub fn has(&self, key: &str) -> bool {
        self.any(key).is_some()
    }

    pub fn string(&self, key: &str) -> Result<Option<String>, RpcError> {
        match self.any(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(RpcError::invalid_params(key, None)),
        }
    }

    pub fn require_string(&self, key: &str) -> Result<String, RpcError> {
        match self.string(key)? {
            Some(s) if !s.trim_matches([' ', '\t']).is_empty() => Ok(s),
            _ => Err(RpcError::invalid_params(
                key,
                Some(&format!("missing required field: {key}")),
            )),
        }
    }

    pub fn int(&self, key: &str) -> Result<Option<i64>, RpcError> {
        match self.any(key) {
            None => Ok(None),
            Some(v) => int_value(v)
                .map(Some)
                .ok_or_else(|| RpcError::invalid_params(key, None)),
        }
    }

    pub fn bool(&self, key: &str) -> Result<Option<bool>, RpcError> {
        match self.any(key) {
            None => Ok(None),
            Some(v) => bool_value(v)
                .map(Some)
                .ok_or_else(|| RpcError::invalid_params(key, None)),
        }
    }

    pub fn dict(&self, key: &str) -> Result<Option<Map<String, Value>>, RpcError> {
        match self.any(key) {
            None => Ok(None),
            Some(Value::Object(m)) => Ok(Some(m.clone())),
            Some(_) => Err(RpcError::invalid_params(key, None)),
        }
    }

    pub fn array(&self, key: &str) -> Result<Option<Vec<Value>>, RpcError> {
        match self.any(key) {
            None => Ok(None),
            Some(Value::Array(a)) => Ok(Some(a.clone())),
            Some(_) => Err(RpcError::invalid_params(key, None)),
        }
    }

    pub fn string_array(&self, key: &str) -> Result<Option<Vec<String>>, RpcError> {
        let Some(a) = self.array(key)? else {
            return Ok(None);
        };
        let mut out = Vec::with_capacity(a.len());
        for item in a {
            match item {
                Value::String(s) => out.push(s),
                _ => return Err(RpcError::invalid_params(key, None)),
            }
        }
        Ok(Some(out))
    }

    /// Flatten `config` values to rclone `key=value` arguments (sorted keys).
    pub fn config_kvs(&self, key: &str) -> Result<Vec<String>, RpcError> {
        let Some(d) = self.dict(key)? else {
            return Ok(Vec::new());
        };
        let mut keys: Vec<&String> = d.keys().collect();
        keys.sort();
        Ok(keys
            .into_iter()
            .map(|k| format!("{k}={}", string_of(d.get(k))))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(v: Value) -> Params {
        Params::new(&v).unwrap()
    }

    #[test]
    fn aliases_resolve() {
        let x = p(json!({"flowId": "a", "canvasJson": "{}", "operationIds": ["1"]}));
        assert_eq!(x.string("flowID").unwrap().as_deref(), Some("a"));
        assert_eq!(x.string("canvasJSON").unwrap().as_deref(), Some("{}"));
        assert_eq!(x.string_array("operationIDs").unwrap().unwrap(), vec!["1"]);
    }

    #[test]
    fn type_errors() {
        let x = p(json!({"n": "abc", "s": 3, "b": "maybe", "d": [], "nul": null}));
        assert_eq!(x.int("n").unwrap_err().code, "invalid_params");
        assert_eq!(x.string("s").unwrap_err().data.unwrap()["field"], "s");
        assert!(x.bool("b").is_err());
        assert!(x.dict("d").is_err());
        assert!(!x.has("nul"));
        assert!(Params::new(&json!([1])).is_err());
        assert!(Params::new(&Value::Null).unwrap().0.is_empty());
    }

    #[test]
    fn require_string_rejects_blank() {
        let x = p(json!({"a": "  ", "b": "ok"}));
        let e = x.require_string("a").unwrap_err();
        assert_eq!(e.message, "missing required field: a");
        assert_eq!(x.require_string("b").unwrap(), "ok");
        assert!(x.require_string("zz").is_err());
    }

    #[test]
    fn int_coercions_and_config_kvs() {
        let x = p(json!({"i": "12", "c": {"b": 1, "a": "x", "t": true}}));
        assert_eq!(x.int("i").unwrap(), Some(12));
        assert_eq!(x.config_kvs("c").unwrap(), vec!["a=x", "b=1", "t=true"]);
        assert!(x.config_kvs("none").unwrap().is_empty());
    }
}
