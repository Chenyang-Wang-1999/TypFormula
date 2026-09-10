//! Bounded backend cache of successful and failed empty-string instances.
use crate::services::{Services, RenderRequest};
use serde_json::{Value, json};

impl Services {
    pub fn prewarm(&self, body: Value) -> Result<Value,String> {
        let source = body["source"].as_str().ok_or("缺少源码")?;
        let path = body["path"].as_str().unwrap_or("main.typ");
        crate::workspace::resolve(&self.workspace, path)?;
        let mut overlays = body.get("overlays").cloned().unwrap_or(json!({}));
        if let Some(overlays)=overlays.as_object_mut() { overlays.remove(path); }
        let mut results = vec![];
        let mut cache = self.warmups.lock().map_err(|e|e.to_string())?;
        for plan in crate::prewarm::plans(source) {
            let key = json!([path, plan.key, overlays]).to_string();
            if let Some(result) = cache.get(&key) { results.push(result.clone()); continue; }
            let mut request = plan.request;
            if let Some(error)=request["warmup_error"].as_str() {
                results.push(json!({"key":plan.key,"failed":true,"error":error,"items":[]}));continue;
            }
            request["path"] = json!(path); request["overlays"] = overlays.clone();
            let request: RenderRequest = serde_json::from_value(request).map_err(|e|e.to_string())?;
            let projection = request.source.clone();
            let expected:Vec<_>=request.raw.iter().map(|r|format!("{}:",r.id)).collect();
            let result = match self.render(request) {
                Ok(mut rendered) => {
                    let missing=expected.iter().any(|prefix|!rendered["items"].as_array().is_some_and(|items|items.iter().any(|item|item["id"].as_str().is_some_and(|id|id.starts_with(prefix)))));
                    if let Some(items) = rendered["items"].as_array_mut() { for item in items {
                        let start=item["start"].as_u64().unwrap_or(0) as usize;
                        let end=item["end"].as_u64().unwrap_or(0) as usize;
                        item["template_source"]=json!(projection.get(start..end));
                    } }
                    if missing { json!({"key":plan.key,"failed":true,"error":"预热实例没有产生全部固定 Raw 的排版结果","items":[]}) }
                    else { json!({"key":plan.key,"failed":false,"items":rendered["items"]}) }
                },
                Err(error) => json!({"key":plan.key,"failed":true,"error":error,"items":[]}),
            };
            if cache.len() >= 128 { cache.clear(); }
            cache.insert(key, result.clone()); results.push(result);
        }
        Ok(json!({"results":results}))
    }
}
