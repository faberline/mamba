//! Core API application model.
//!
//! `App` is an httpkit-native route registry and host contract target. It is
//! not an ASGI/WSGI callable; native host code dispatches directly into this
//! model.

mod validation;

use self::validation::validate_json_schema_value;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#route
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub name: Option<String>,
}

impl Route {
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: normalize_path(&path.into()),
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Native endpoint metadata shared by HTTP routing, DI, schema validation,
/// OpenAPI generation, and host dispatch.
///
/// This is an extension contract for `mambalibs.http`. It intentionally does
/// not change CPython stdlib HTTP behavior or Python syntax; later decorator
/// lowering can populate this from ordinary `@app.post(...)`, parameter
/// defaults, and annotations.
/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#endpoint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub handler_name: Option<String>,
    pub dependency_keys: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<RouteParameter>,
    pub request_model: Option<String>,
    pub request_schema: Option<String>,
    pub response_model: Option<String>,
    pub response_schema: Option<String>,
    pub status_code: u16,
}

impl Endpoint {
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: normalize_method(&method.into()),
            path: normalize_path(&path.into()),
            handler_name: None,
            dependency_keys: Vec::new(),
            parameters: Vec::new(),
            request_model: None,
            request_schema: None,
            response_model: None,
            response_schema: None,
            status_code: 200,
        }
    }

    pub fn with_handler_name(mut self, handler_name: impl Into<String>) -> Self {
        self.handler_name = non_empty(handler_name.into());
        self
    }

    pub fn with_dependency_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            self.dependency_keys.push(trimmed.to_string());
        }
        self
    }

    pub fn with_dependency_keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for key in keys {
            self = self.with_dependency_key(key);
        }
        self
    }

    pub fn with_parameter(mut self, parameter: RouteParameter) -> Self {
        if !parameter.name.trim().is_empty() {
            self.parameters.push(parameter);
        }
        self
    }

    pub fn with_parameters<I>(mut self, parameters: I) -> Self
    where
        I: IntoIterator<Item = RouteParameter>,
    {
        for parameter in parameters {
            self = self.with_parameter(parameter);
        }
        self
    }

    pub fn with_request_model(mut self, model: impl Into<String>) -> Self {
        self.request_model = non_empty(model.into());
        self
    }

    pub fn with_request_schema_json(mut self, schema_json: impl Into<String>) -> Self {
        self.request_schema = non_empty(schema_json.into());
        self
    }

    pub fn with_response_model(mut self, model: impl Into<String>) -> Self {
        self.response_model = non_empty(model.into());
        self
    }

    pub fn with_response_schema_json(mut self, schema_json: impl Into<String>) -> Self {
        self.response_schema = non_empty(schema_json.into());
        self
    }

    pub fn with_status_code(mut self, status_code: u16) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn route(&self) -> Route {
        let mut route = Route::new(self.method.clone(), self.path.clone());
        route.name = self.handler_name.clone();
        route
    }
}

/// FastAPI-style non-body route parameter metadata for OpenAPI and preflight.
///
/// This is an additive `mambalibs.http` contract and does not alter CPython
/// stdlib HTTP behavior.
/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#param
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParameter {
    pub name: String,
    pub location: String,
    pub required: bool,
    pub description: Option<String>,
    pub default_json: Option<String>,
    pub schema_json: Option<String>,
}

impl RouteParameter {
    pub fn query(name: impl Into<String>) -> Self {
        Self::new(name, "query")
    }

    pub fn header(name: impl Into<String>) -> Self {
        Self::new(name, "header")
    }

    pub fn new(name: impl Into<String>, location: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            location: normalize_parameter_location(&location.into()),
            required: true,
            description: None,
            default_json: None,
            schema_json: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = non_empty(description.into());
        self
    }

    pub fn default_json(mut self, default_json: impl Into<String>) -> Self {
        self.default_json = non_empty(default_json.into());
        if self.default_json.is_some() {
            self.required = false;
        }
        self
    }

    pub fn schema_json(mut self, schema_json: impl Into<String>) -> Self {
        self.schema_json = non_empty(schema_json.into());
        self
    }
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#router
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Router {
    pub prefix: String,
    pub tags: Vec<String>,
    pub routes: Vec<Route>,
    pub endpoints: Vec<Endpoint>,
}

impl Router {
    pub fn new(prefix: impl Into<String>, tags: Vec<String>) -> Self {
        Self {
            prefix: normalize_prefix(&prefix.into()),
            tags,
            routes: Vec::new(),
            endpoints: Vec::new(),
        }
    }

    pub fn add_route(&mut self, method: impl Into<String>, path: impl Into<String>) {
        self.routes.push(Route::new(method, path));
    }

    pub fn add_endpoint(&mut self, endpoint: Endpoint) {
        self.routes.push(endpoint.route());
        self.endpoints.push(endpoint);
    }

    pub fn full_path(&self, route: &Route) -> String {
        join_paths(&self.prefix, &route.path)
    }

    pub fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#app
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub metadata: HashMap<String, String>,
    pub router: Router,
    pub middleware: Vec<Middleware>,
    pub mounts: Vec<StaticFiles>,
}

impl App {
    pub fn new(metadata: HashMap<String, String>) -> Self {
        Self {
            metadata,
            router: Router::new(String::new(), Vec::new()),
            middleware: Vec::new(),
            mounts: Vec::new(),
        }
    }

    pub fn add_route(&mut self, method: impl Into<String>, path: impl Into<String>) {
        self.router.add_route(method, path);
    }

    pub fn add_endpoint(&mut self, endpoint: Endpoint) {
        self.router.add_endpoint(endpoint);
    }

    pub fn route_count(&self) -> usize {
        self.router.routes.len()
    }

    pub fn endpoint_count(&self) -> usize {
        self.router.endpoint_count()
    }

    pub fn endpoints(&self) -> &[Endpoint] {
        &self.router.endpoints
    }

    pub fn openapi_json(&self) -> String {
        serde_json::to_string(&self.openapi_value())
            .unwrap_or_else(|_| "{\"openapi\":\"3.1.0\",\"paths\":{}}".to_string())
    }

    pub fn preflight_request_json(
        &self,
        method: &str,
        path: &str,
        body: Value,
        dependencies: HashMap<String, String>,
    ) -> String {
        self.preflight_request_json_with_context(
            method,
            path,
            body,
            dependencies,
            HashMap::new(),
            HashMap::new(),
        )
    }

    pub fn preflight_request_json_with_context(
        &self,
        method: &str,
        path: &str,
        body: Value,
        dependencies: HashMap<String, String>,
        query: HashMap<String, String>,
        headers: HashMap<String, String>,
    ) -> String {
        serde_json::to_string(&self.preflight_request_value(
            method,
            path,
            body,
            dependencies,
            query,
            headers,
        ))
        .unwrap_or_else(|_| "{\"matched\":false,\"status_code\":500}".to_string())
    }

    fn preflight_request_value(
        &self,
        method: &str,
        path: &str,
        body: Value,
        dependencies: HashMap<String, String>,
        query: HashMap<String, String>,
        headers: HashMap<String, String>,
    ) -> Value {
        let normalized_method = normalize_method(method);
        let normalized_path = normalize_path(path);
        let Some(endpoint) = self.endpoints().iter().find(|endpoint| {
            endpoint.method == normalized_method && endpoint.path == normalized_path
        }) else {
            return json!({
                "matched": false,
                "status_code": 404,
                "method": normalized_method,
                "path": normalized_path,
                "body": body,
                "errors": ["route not found"]
            });
        };

        let mut errors = Vec::new();
        if let Some(schema_json) = &endpoint.request_schema {
            match serde_json::from_str::<Value>(schema_json) {
                Ok(schema) => validate_json_schema_value(&body, &schema, "$", &mut errors),
                Err(err) => errors.push(format!("request schema parse error: {err}")),
            }
        }
        let (parameters, parameter_errors, detail) =
            validate_route_parameters(endpoint, &query, &headers);
        errors.extend(parameter_errors);

        let mut resolved = Map::new();
        let mut dependency_errors = Vec::new();
        for key in &endpoint.dependency_keys {
            match dependencies.get(key) {
                Some(value) => {
                    resolved.insert(key.clone(), json!(value));
                }
                None => dependency_errors.push(format!("dependency not resolved: {key}")),
            }
        }

        let status_code = if !errors.is_empty() {
            422
        } else if !dependency_errors.is_empty() {
            500
        } else {
            endpoint.status_code
        };

        json!({
            "matched": true,
            "status_code": status_code,
            "method": endpoint.method,
            "path": endpoint.path,
            "request_model": endpoint.request_model,
            "response_model": endpoint.response_model,
            "body": body,
            "parameters": Value::Object(parameters),
            "dependencies": Value::Object(resolved),
            "errors": errors,
            "detail": detail,
            "dependency_errors": dependency_errors
        })
    }

    fn openapi_value(&self) -> Value {
        let title = self
            .metadata
            .get("title")
            .cloned()
            .unwrap_or_else(|| "Mamba API".to_string());
        let version = self
            .metadata
            .get("version")
            .cloned()
            .unwrap_or_else(|| "0.1.0".to_string());

        let mut paths = Map::new();
        let mut schemas = Map::new();

        for endpoint in self.endpoints() {
            let mut operation = Map::new();
            if let Some(name) = &endpoint.handler_name {
                operation.insert("operationId".to_string(), json!(name));
            }
            if !endpoint.dependency_keys.is_empty() {
                operation.insert(
                    "x-mamba-dependencies".to_string(),
                    json!(endpoint.dependency_keys),
                );
            }
            if !endpoint.parameters.is_empty() {
                operation.insert(
                    "parameters".to_string(),
                    Value::Array(
                        endpoint
                            .parameters
                            .iter()
                            .map(parameter_openapi_value)
                            .collect(),
                    ),
                );
            }
            if let Some(model) = &endpoint.request_model {
                insert_schema_component(&mut schemas, model, endpoint.request_schema.as_deref());
                operation.insert(
                    "requestBody".to_string(),
                    json!({
                        "content": {
                            "application/json": {
                                "schema": model_ref(model)
                            }
                        }
                    }),
                );
            }

            let response_status = endpoint.status_code.to_string();
            let mut response = Map::new();
            response.insert(
                "description".to_string(),
                json!(if endpoint.status_code < 400 {
                    "Successful Response"
                } else {
                    "Error Response"
                }),
            );
            if let Some(model) = &endpoint.response_model {
                insert_schema_component(&mut schemas, model, endpoint.response_schema.as_deref());
                response.insert(
                    "content".to_string(),
                    json!({
                        "application/json": {
                            "schema": model_ref(model)
                        }
                    }),
                );
            }
            let mut responses = Map::new();
            responses.insert(response_status, Value::Object(response));
            operation.insert("responses".to_string(), Value::Object(responses));

            let path_entry = paths
                .entry(endpoint.path.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(methods) = path_entry {
                methods.insert(
                    endpoint.method.to_ascii_lowercase(),
                    Value::Object(operation),
                );
            }
        }

        json!({
            "openapi": "3.1.0",
            "info": {
                "title": title,
                "version": version
            },
            "paths": Value::Object(paths),
            "components": {
                "schemas": Value::Object(schemas)
            }
        })
    }
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#middleware
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Middleware {
    pub name: String,
    pub options: HashMap<String, String>,
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#middleware
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CORSMiddleware {
    pub allow_origins: Vec<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub allow_credentials: bool,
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#static-files
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticFiles {
    pub directory: Option<String>,
    pub html: bool,
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#param
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterKind {
    Depends,
    Query,
    Body,
    Header,
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#param
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub kind: ParameterKind,
    pub alias: Option<String>,
    pub required: bool,
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#background-tasks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub name: String,
    pub payload_json: Option<String>,
    pub queue: Option<String>,
}

impl BackgroundTask {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            payload_json: None,
            queue: None,
        }
    }

    pub fn payload_json(mut self, payload_json: impl Into<String>) -> Self {
        self.payload_json = non_empty(payload_json.into());
        self
    }

    pub fn queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = non_empty(queue.into());
        self
    }

    fn to_value(&self) -> Value {
        let payload = self
            .payload_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(Value::Null);
        json!({
            "name": self.name,
            "payload": payload,
            "queue": self.queue,
        })
    }
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#background-tasks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTasks {
    pub task_count: usize,
    pub tasks: Vec<BackgroundTask>,
}

impl BackgroundTasks {
    pub fn new() -> Self {
        Self {
            task_count: 0,
            tasks: Vec::new(),
        }
    }

    pub fn add_task(&mut self, task: BackgroundTask) -> bool {
        if task.name.is_empty() {
            return false;
        }
        self.tasks.push(task);
        self.task_count = self.tasks.len();
        true
    }

    pub fn add_named_task(
        &mut self,
        name: impl Into<String>,
        payload_json: Option<String>,
        queue: Option<String>,
    ) -> bool {
        let mut task = BackgroundTask::new(name);
        if let Some(payload_json) = payload_json {
            task = task.payload_json(payload_json);
        }
        if let Some(queue) = queue {
            task = task.queue(queue);
        }
        self.add_task(task)
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn drain(&mut self) -> Vec<BackgroundTask> {
        let tasks = std::mem::take(&mut self.tasks);
        self.task_count = 0;
        tasks
    }

    pub fn tasks_value(&self) -> Value {
        Value::Array(self.tasks.iter().map(BackgroundTask::to_value).collect())
    }

    pub fn tasks_json(&self) -> String {
        serde_json::to_string(&self.tasks_value()).unwrap_or_else(|_| "[]".to_string())
    }
}

impl Default for BackgroundTasks {
    fn default() -> Self {
        Self::new()
    }
}

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#request-context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestContext;

/// @spec .score/tech_design/projects/httpkit/runtime-surface.md#streaming-response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingResponse {
    pub status_code: u16,
    pub media_type: Option<String>,
    pub headers: HashMap<String, String>,
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim();
    if trimmed.is_empty() || trimmed == "/" {
        String::new()
    } else {
        normalize_path(trimmed).trim_end_matches('/').to_string()
    }
}

fn normalize_method(method: &str) -> String {
    let trimmed = method.trim();
    if trimmed.is_empty() {
        "GET".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_parameter_location(location: &str) -> String {
    match location.trim().to_ascii_lowercase().as_str() {
        "header" => "header".to_string(),
        _ => "query".to_string(),
    }
}

fn model_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn parameter_openapi_value(parameter: &RouteParameter) -> Value {
    let mut parameter_doc = Map::new();
    parameter_doc.insert("name".to_string(), json!(parameter.name));
    parameter_doc.insert("in".to_string(), json!(parameter.location));
    parameter_doc.insert("required".to_string(), json!(parameter.required));
    if let Some(description) = &parameter.description {
        parameter_doc.insert("description".to_string(), json!(description));
    }

    let mut schema = parameter
        .schema_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({ "type": "string" }));
    if let (Some(default), Value::Object(schema_object)) = (
        parameter
            .default_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
        &mut schema,
    ) {
        schema_object.insert("default".to_string(), default);
    }
    parameter_doc.insert("schema".to_string(), schema);
    Value::Object(parameter_doc)
}

fn validate_route_parameters(
    endpoint: &Endpoint,
    query: &HashMap<String, String>,
    headers: &HashMap<String, String>,
) -> (Map<String, Value>, Vec<String>, Vec<Value>) {
    let mut values = Map::new();
    let mut errors = Vec::new();
    let mut detail = Vec::new();

    for parameter in &endpoint.parameters {
        let raw = match parameter.location.as_str() {
            "header" => lookup_header(headers, &parameter.name),
            _ => query.get(&parameter.name).cloned(),
        };

        if let Some(value) = raw {
            values.insert(parameter.name.clone(), Value::String(value));
            continue;
        }

        if let Some(default) = parameter
            .default_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        {
            values.insert(parameter.name.clone(), default);
            continue;
        }

        if parameter.required {
            errors.push(format!(
                "{}.{}: missing required parameter",
                parameter.location, parameter.name
            ));
            detail.push(json!({
                "loc": [parameter.location, parameter.name],
                "msg": "Field required",
                "type": "missing",
            }));
        }
    }

    (values, errors, detail)
}

fn lookup_header(headers: &HashMap<String, String>, name: &str) -> Option<String> {
    headers.get(name).cloned().or_else(|| {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    })
}

fn insert_schema_component(
    schemas: &mut Map<String, Value>,
    name: &str,
    schema_json: Option<&str>,
) {
    if schemas.contains_key(name) {
        return;
    }
    let schema = schema_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({ "title": name, "type": "object" }));
    schemas.insert(name.to_string(), schema);
}

fn join_paths(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        normalize_path(path)
    } else if path == "/" {
        prefix.to_string()
    } else {
        format!("{}{}", prefix.trim_end_matches('/'), normalize_path(path))
    }
}
