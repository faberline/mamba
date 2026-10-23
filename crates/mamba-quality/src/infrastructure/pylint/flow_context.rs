//! Exception and finally context belongs to the source statement and lexical frame.
use super::{display, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, ExprCall, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Handler {
    type_name: String,
    name: Option<String>,
}
#[derive(Clone, Copy)]
pub(super) struct CallContext {
    pub statement: u32,
    pub lambda: bool,
    pub awaited: bool,
}

#[derive(Default)]
pub(super) struct Contexts {
    handlers: BTreeMap<u32, Handler>,
    bare_raises: BTreeSet<u32>,
    lost_returns: BTreeSet<u32>,
    finally_returns: BTreeMap<u32, Vec<TextRange>>,
    pub calls: BTreeMap<u32, CallContext>,
    lost_breaks: BTreeSet<u32>,
    outside_loop: BTreeSet<u32>,
    direct_final: BTreeSet<u32>,
}

struct Collector<'a> {
    source: &'a str,
    data: Contexts,
    handler: Option<Handler>,
    final_blocks: Vec<u32>,
    lost: bool,
    lost_break: bool,
    exit_method: bool,
    in_class: bool,
    loops: usize,
    statement: u32,
    parent_lambda: bool,
    parent_await: bool,
    direct_final: bool,
}

impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let previous_statement = self.statement;
        self.statement = statement.start().to_u32();
        match statement {
            Stmt::Return(returned) => {
                for block in &self.final_blocks {
                    self.data
                        .finally_returns
                        .entry(*block)
                        .or_default()
                        .push(returned.range());
                }
                if self.lost {
                    self.data.lost_returns.insert(returned.start().to_u32());
                }
            }
            Stmt::Raise(raised) => {
                if let Some(handler) = &self.handler {
                    self.data
                        .handlers
                        .insert(raised.start().to_u32(), handler.clone());
                }
                if raised.exc.is_none() && self.handler.is_none() && !self.exit_method {
                    self.data.bare_raises.insert(raised.start().to_u32());
                }
            }
            Stmt::FunctionDef(function) => {
                let previous = (
                    self.handler.take(),
                    self.lost,
                    self.lost_break,
                    self.exit_method,
                    self.loops,
                    self.in_class,
                );
                self.lost = false;
                self.lost_break = false;
                self.exit_method = self.in_class && function.name.as_str() == "__exit__";
                self.in_class = false;
                self.loops = 0;
                self.visit_body(&function.body);
                (
                    self.handler,
                    self.lost,
                    self.lost_break,
                    self.exit_method,
                    self.loops,
                    self.in_class,
                ) = previous;
                self.statement = previous_statement;
                return;
            }
            Stmt::ClassDef(class) => {
                let previous = (self.in_class, self.loops);
                self.in_class = true;
                self.loops = 0;
                self.visit_body(&class.body);
                (self.in_class, self.loops) = previous;
                self.statement = previous_statement;
                return;
            }
            Stmt::Break(_) | Stmt::Continue(_) => {
                if self.loops == 0 {
                    self.data.outside_loop.insert(statement.start().to_u32());
                }
                if self.direct_final {
                    self.data.direct_final.insert(statement.start().to_u32());
                }
                if self.lost_break && matches!(statement, Stmt::Break(_)) {
                    self.data.lost_breaks.insert(statement.start().to_u32());
                }
            }
            Stmt::For(loop_) => {
                self.visit_expr(&loop_.target);
                self.visit_expr(&loop_.iter);
                self.loop_body(&loop_.body, &loop_.orelse);
                self.statement = previous_statement;
                return;
            }
            Stmt::While(loop_) => {
                self.visit_expr(&loop_.test);
                self.loop_body(&loop_.body, &loop_.orelse);
                self.statement = previous_statement;
                return;
            }
            Stmt::Try(attempt) => {
                self.visit_body(&attempt.body);
                for handler in &attempt.handlers {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    let previous = self.handler.replace(Handler {
                        type_name: handler
                            .type_
                            .as_deref()
                            .filter(|value| matches!(value, Expr::Name(_) | Expr::Tuple(_)))
                            .map(|value| display::expression(value, self.source))
                            .unwrap_or_else(|| "Exception".into()),
                        name: handler.name.as_ref().map(ToString::to_string),
                    });
                    self.visit_body(&handler.body);
                    self.handler = previous;
                }
                self.visit_body(&attempt.orelse);
                self.final_blocks.push(attempt.start().to_u32());
                let previous = self.lost;
                let previous_break = self.lost_break;
                let previous_final = self.direct_final;
                self.lost = true;
                self.lost_break = true;
                for statement in &attempt.finalbody {
                    self.direct_final = matches!(statement, Stmt::Break(_) | Stmt::Continue(_));
                    self.visit_stmt(statement);
                }
                self.lost = previous;
                self.lost_break = previous_break;
                self.direct_final = previous_final;
                self.final_blocks.pop();
                self.statement = previous_statement;
                return;
            }
            _ => {}
        }
        walk_stmt(self, statement);
        self.statement = previous_statement;
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Call(call) = value {
            self.data.calls.insert(
                call.start().to_u32(),
                CallContext {
                    statement: self.statement,
                    lambda: self.parent_lambda,
                    awaited: self.parent_await,
                },
            );
        }
        let previous = (self.parent_lambda, self.parent_await);
        self.parent_lambda = matches!(value, Expr::Lambda(_));
        self.parent_await = matches!(value, Expr::Await(_));
        walk_expr(self, value);
        (self.parent_lambda, self.parent_await) = previous;
    }
}

impl Collector<'_> {
    fn loop_body<'a>(&mut self, body: &'a [Stmt], orelse: &'a [Stmt]) {
        let previous = self.lost_break;
        self.lost_break = false;
        self.loops += 1;
        self.visit_body(body);
        self.loops -= 1;
        self.lost_break = previous;
        self.visit_body(orelse);
    }
}

impl Contexts {
    pub(super) fn new(body: &[Stmt], source: &str) -> Self {
        let mut collector = Collector {
            source,
            data: Self::default(),
            handler: None,
            final_blocks: Vec::new(),
            lost: false,
            lost_break: false,
            exit_method: false,
            in_class: false,
            loops: 0,
            statement: 0,
            parent_lambda: false,
            parent_await: false,
            direct_final: false,
        };
        collector.visit_body(body);
        collector.data
    }
}

impl Checker<'_> {
    pub(super) fn check_flow_call(&mut self, call: &ExprCall) {
        let Some(context) = self.flow.calls.get(&call.start().to_u32()).copied() else {
            return;
        };
        if self.terminating_call(call, true) {
            if let Some(next) = self
                .statements
                .contexts
                .get(&context.statement)
                .and_then(|value| value.next)
            {
                self.emit("W0101", "unreachable", "Unreachable code", next);
            }
        }
    }
    pub(super) fn check_flow_context(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Try(attempt) => {
                let ranges = self
                    .flow
                    .finally_returns
                    .get(&attempt.start().to_u32())
                    .cloned()
                    .unwrap_or_default();
                for range in ranges {
                    self.emit(
                        "W0134",
                        "return-in-finally",
                        "'return' shadowed by the 'finally' clause.",
                        range,
                    );
                }
            }
            Stmt::Return(returned)
                if self.flow.lost_returns.contains(&returned.start().to_u32()) =>
            {
                self.emit(
                    "W0150",
                    "lost-exception",
                    "return statement in finally block may swallow exception",
                    returned.range(),
                )
            }
            Stmt::Raise(raised) => {
                if self.flow.bare_raises.contains(&raised.start().to_u32()) {
                    self.emit(
                        "E0704",
                        "misplaced-bare-raise",
                        "The raise statement is not inside an except clause",
                        raised.range(),
                    );
                } else if raised.exc.is_some() && raised.cause.is_none() {
                    if let Some(handler) = self.flow.handlers.get(&raised.start().to_u32()) {
                        let violation = handler.name.is_none()
                            || matches!(raised.exc.as_deref(), Some(Expr::Call(call)) if matches!(call.func.as_ref(), Expr::Name(_)))
                            || matches!(raised.exc.as_deref(), Some(Expr::Name(name)) if handler.name.as_deref() != Some(name.id.as_str()));
                        if violation {
                            let prefix = if handler.name.is_none() {
                                format!("'except {} as exc' and ", handler.type_name)
                            } else {
                                String::new()
                            };
                            let name = handler.name.as_deref().unwrap_or("exc");
                            let expression = display::expression(
                                raised.exc.as_deref().unwrap(),
                                &self.source.text,
                            );
                            self.emit("W0707", "raise-missing-from", &format!("Consider explicitly re-raising using {prefix}'raise {expression} from {name}'"), raised.range());
                        }
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) => {
                let name = if matches!(statement, Stmt::Break(_)) {
                    "break"
                } else {
                    "continue"
                };
                let key = statement.start().to_u32();
                if self.flow.lost_breaks.contains(&key) {
                    self.emit(
                        "W0150",
                        "lost-exception",
                        "break statement in finally block may swallow exception",
                        statement.range(),
                    );
                }
                if self.flow.direct_final.contains(&key) {
                    self.emit(
                        if name == "break" { "W0137" } else { "W0136" },
                        if name == "break" {
                            "break-in-finally"
                        } else {
                            "continue-in-finally"
                        },
                        &format!("'{name}' discouraged inside 'finally' clause"),
                        statement.range(),
                    );
                }
                if self.flow.outside_loop.contains(&key) {
                    self.emit(
                        "E0103",
                        "not-in-loop",
                        &format!("'{name}' not properly in loop"),
                        statement.range(),
                    );
                }
            }
            _ => {}
        }
    }
}
