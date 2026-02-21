mod parser;

use parser::{
    Command, Expression, FormatString, FormatStringPart, Node, ProgramPart, Value, parse_program,
};
use serde::Serialize;
use std::{collections::HashMap, fmt::Display, time::Instant};

#[derive(Debug)]
pub enum ParseError<'a> {
    MissingStartNode,
    BadReferenceInOption {
        parent_node_id: String,
        bad_id: String,
    },
    BadReferenceInString {
        parent_node_id: String,
        bad_name: String,
    },
    BadReferenceInExpression {
        parent_node_id: String,
        bad_name: String,
    },
    InvalidExpression {
        parent_node_id: String,
        expression: Expression<'a>,
    },
    BadReferenceInCommand {
        parent_node_id: String,
        bad_name: String,
    },
    InvalidCommand {
        parent_node_id: String,
        command: Command<'a>,
    },
}

impl Display for ParseError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingStartNode => f.write_fmt(format_args!("Your program is missing a 'START' node, which is required as the entry point of the game.")),
            Self::BadReferenceInOption { parent_node_id, bad_id } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains an option that references a non-existent node with id '{bad_id}'.")),
            Self::BadReferenceInString { parent_node_id, bad_name } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains a string that references a non-existent variable with name '{bad_name}'.")),
            Self::BadReferenceInExpression { parent_node_id, bad_name } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains an expression that references a non-existent variable with name '{bad_name}'.")),
            Self::InvalidExpression { parent_node_id, expression } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains an expression that is invalid: {expression}.")),
            Self::BadReferenceInCommand { parent_node_id, bad_name } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains a command that references a non-existent variable with name '{bad_name}'.")),
            Self::InvalidCommand { parent_node_id, command } => f.write_fmt(format_args!("The node with id '{parent_node_id}' contains a command that is invalid: '{command}'.")),
        }
    }
}

#[derive(Serialize)]
pub struct ChoiceView {
    pub display_text: String,
    pub id: String,
}

#[derive(Serialize)]
pub struct CurrentNodeView {
    pub display_text: String,
    pub choices: Vec<ChoiceView>,
    pub game_over: bool,
}

#[derive(Serialize)]
pub enum ChoiceResult {
    Success,
    InvalidOption {
        current_node_id: String,
        chosen_option: String,
    },
}

/// Per-session mutable game state.
pub struct Session {
    created_at: Instant,
    variables: HashMap<String, Value>,
    current_node_id: String,
}

impl Session {
    // sessions expire after 24 hours, at which point they should be deleted.
    pub fn is_expired(&self) -> bool {
        let hours = self.created_at.elapsed().as_secs() / 60 / 60;

        hours >= 24
    }
}

/// Shared, immutable story data. Loaded once at startup and referenced by all sessions.
pub struct Engine<'a> {
    default_variables: HashMap<String, Value>,
    all_nodes: HashMap<String, Node<'a>>,
}

impl<'a> Engine<'a> {
    pub fn new() -> Self {
        Engine {
            default_variables: HashMap::new(),
            all_nodes: HashMap::new(),
        }
    }

    /// Create a fresh session starting at the beginning of the story.
    pub fn new_session(&self) -> Session {
        Session {
            created_at: Instant::now(),
            variables: self.default_variables.clone(),
            current_node_id: "START".to_string(),
        }
    }

    fn bad_names_in_string(&self, s: &FormatString) -> Vec<String> {
        let mut bad_names = Vec::new();
        for part in &s.0 {
            if let FormatStringPart::Name(name) = part {
                if !self.default_variables.contains_key(name) {
                    bad_names.push(name.to_string());
                }
            }
        }
        bad_names
    }

    fn bad_names_in_expression(&self, expr: &Expression) -> Vec<String> {
        let mut bad_names = Vec::new();
        match expr {
            Expression::Value(_) => {}
            Expression::Name(name) => {
                if !self.default_variables.contains_key(name) {
                    bad_names.push(name.to_string());
                }
            }
            Expression::Equals { left, right }
            | Expression::NotEquals { left, right }
            | Expression::GreaterThan { left, right }
            | Expression::LessThan { left, right } => {
                bad_names.extend(self.bad_names_in_expression(left));
                bad_names.extend(self.bad_names_in_expression(right));
            }
        }
        bad_names
    }

    fn expression_is_valid(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Value(_) => true,
            Expression::Name(name) => self.default_variables.contains_key(name),
            Expression::Equals { left, right } | Expression::NotEquals { left, right } => {
                self.expression_is_valid(left) && self.expression_is_valid(right)
            }
            Expression::GreaterThan { left, right } | Expression::LessThan { left, right } => {
                let left_is_int = if let Expression::Value(Value::Int(_)) = left {
                    true
                } else if let Expression::Name(name) = left {
                    matches!(self.default_variables.get(name), Some(Value::Int(_)))
                } else {
                    false
                };
                let right_is_int = if let Expression::Value(Value::Int(_)) = right {
                    true
                } else if let Expression::Name(name) = right {
                    matches!(self.default_variables.get(name), Some(Value::Int(_)))
                } else {
                    false
                };

                if left_is_int && right_is_int {
                    self.expression_is_valid(left) && self.expression_is_valid(right)
                } else {
                    false
                }
            }
        }
    }

    fn bad_names_in_command(&self, command: &Command) -> Vec<String> {
        let mut bad_names = Vec::new();
        match command {
            Command::Set { name, value } => {
                if !self.default_variables.contains_key(*name) {
                    bad_names.push(name.to_string());
                }
                if let Value::String(s) = value {
                    bad_names.extend(self.bad_names_in_string(s));
                }
            }
        }
        bad_names
    }

    fn command_is_valid(&self, command: &Command) -> bool {
        match command {
            Command::Set { name, value } => {
                self.default_variables.contains_key(*name)
                    && match value {
                        Value::Int(_) | Value::Bool(_) => true,
                        Value::String(s) => self.bad_names_in_string(s).is_empty(),
                    }
            }
        }
    }

    fn errors(&self) -> Vec<ParseError<'a>> {
        let mut errors = Vec::new();

        if !self.all_nodes.contains_key("START") {
            errors.push(ParseError::MissingStartNode);
        }

        for (id, node) in self.all_nodes.iter() {
            for name in self.bad_names_in_string(&node.display_text) {
                errors.push(ParseError::BadReferenceInString {
                    parent_node_id: id.to_string(),
                    bad_name: name,
                });
            }

            for choice in &node.choices {
                for name in self.bad_names_in_string(&choice.text) {
                    errors.push(ParseError::BadReferenceInString {
                        parent_node_id: id.to_string(),
                        bad_name: name,
                    });
                }

                let next_node_id = &choice.next_node_id;
                if !self.all_nodes.contains_key(next_node_id.as_str()) {
                    errors.push(ParseError::BadReferenceInOption {
                        parent_node_id: id.to_string(),
                        bad_id: next_node_id.to_string(),
                    });
                }

                if let Some(requirement) = choice.requirement.as_ref() {
                    for name in self.bad_names_in_expression(requirement) {
                        errors.push(ParseError::BadReferenceInExpression {
                            parent_node_id: id.to_string(),
                            bad_name: name,
                        });
                    }

                    if !self.expression_is_valid(requirement) {
                        errors.push(ParseError::InvalidExpression {
                            parent_node_id: id.to_string(),
                            expression: requirement.clone(),
                        });
                    }
                }

                if let Some(command) = choice.command.as_ref() {
                    for name in self.bad_names_in_command(command) {
                        errors.push(ParseError::BadReferenceInCommand {
                            parent_node_id: id.to_string(),
                            bad_name: name,
                        });
                    }

                    if !self.command_is_valid(command) {
                        errors.push(ParseError::InvalidCommand {
                            parent_node_id: id.to_string(),
                            command: command.clone(),
                        });
                    }
                }
            }
        }

        errors
    }

    pub fn from_program(source: &'a str) -> Result<Self, Vec<ParseError<'a>>> {
        let (_, parts) = parse_program(source).expect("Failed to parse nodes");
        let variable_defs: Vec<_> = parts
            .iter()
            .filter(|part| matches!(part, ProgramPart::VariableDefinition { .. }))
            .collect();
        let node_defs: Vec<_> = parts
            .iter()
            .filter(|part| matches!(part, ProgramPart::NodeDefinition { .. }))
            .collect();

        let mut engine = Engine::new();
        for var_def in variable_defs {
            if let ProgramPart::VariableDefinition { name, value } = var_def {
                engine
                    .default_variables
                    .insert(name.to_string(), value.clone());
            } else {
                unreachable!()
            };
        }
        for node_def in node_defs {
            if let ProgramPart::NodeDefinition { id, node } = node_def {
                engine.add_node(id.to_string(), node.clone());
            } else {
                unreachable!()
            };
        }

        let errors = engine.errors();
        if errors.is_empty() {
            Ok(engine)
        } else {
            Err(errors)
        }
    }

    pub fn add_node(&mut self, id: String, node: Node<'a>) {
        self.all_nodes.insert(id, node);
    }

    fn value_to_string(&self, session: &Session, value: &Value) -> String {
        match value {
            Value::Int(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => self.evaluate_string(session, s),
        }
    }

    fn evaluate_string(&self, session: &Session, input: &FormatString) -> String {
        let mut result = String::new();
        for part in &input.0 {
            match part {
                FormatStringPart::Literal(s) => result.push_str(s),
                FormatStringPart::Name(name) => {
                    let var_value = session
                        .variables
                        .get(name)
                        .map(|v| self.value_to_string(session, v))
                        .unwrap();
                    result.push_str(var_value.as_str());
                }
            }
        }

        result
    }

    fn values_are_equal(&self, session: &Session, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(l), Value::Int(r)) => l == r,
            (Value::Bool(l), Value::Bool(r)) => l == r,
            (Value::String(l), Value::String(r)) => {
                self.evaluate_string(session, l) == self.evaluate_string(session, r)
            }
            _ => false,
        }
    }

    fn evaluate_expression(&self, session: &Session, input: &Expression) -> Value {
        match input {
            Expression::Value(v) => v.clone(),
            Expression::Name(name) => session.variables.get(name).unwrap().clone(),
            Expression::Equals { left, right } => {
                let left_val = self.evaluate_expression(session, left);
                let right_val = self.evaluate_expression(session, right);
                Value::Bool(self.values_are_equal(session, &left_val, &right_val))
            }
            Expression::NotEquals { left, right } => {
                let left_val = self.evaluate_expression(session, left);
                let right_val = self.evaluate_expression(session, right);
                Value::Bool(!self.values_are_equal(session, &left_val, &right_val))
            }
            Expression::GreaterThan { left, right } => {
                let left_val = self.evaluate_expression(session, left);
                let right_val = self.evaluate_expression(session, right);
                match (left_val, right_val) {
                    (Value::Int(l), Value::Int(r)) => Value::Bool(l > r),
                    _ => panic!("GreaterThan operator can only be applied to integers"),
                }
            }
            Expression::LessThan { left, right } => {
                let left_val = self.evaluate_expression(session, left);
                let right_val = self.evaluate_expression(session, right);
                match (left_val, right_val) {
                    (Value::Int(l), Value::Int(r)) => Value::Bool(l < r),
                    _ => panic!("LessThan operator can only be applied to integers"),
                }
            }
        }
    }

    fn get_current_node<'b>(&'b self, session: &Session) -> &'b Node<'a> {
        self.all_nodes
            .get(session.current_node_id.as_str())
            .unwrap()
    }

    pub fn get_valid_options_ids(&self, session: &Session) -> Vec<&str> {
        self.get_current_node(session)
            .choices
            .iter()
            .map(|choice| choice.next_node_id.as_str())
            .collect()
    }

    pub fn get_current_node_view(&self, session: &Session) -> CurrentNodeView {
        let current_node = self.get_current_node(session);

        let display_text = self.evaluate_string(session, &current_node.display_text);
        let choices = current_node
            .choices
            .iter()
            .filter_map(|choice| {
                if let Some(req) = &choice.requirement {
                    if !self.evaluate_expression(session, req).is_truthy() {
                        return None;
                    }
                }

                Some(ChoiceView {
                    id: choice.next_node_id.to_string(),
                    display_text: self.evaluate_string(session, &choice.text),
                })
            })
            .collect();
        let game_over = current_node.choices.is_empty();

        CurrentNodeView {
            display_text,
            choices,
            game_over,
        }
    }

    fn do_command(&self, session: &mut Session, command: &Command) {
        match command {
            Command::Set { name, value } => {
                let var = session.variables.get_mut(*name).unwrap();
                *var = value.clone();
            }
        }
    }

    pub fn choose_option(&self, session: &mut Session, next_node_id: String) -> ChoiceResult {
        let valid_options = self.get_valid_options_ids(session);
        if !valid_options.contains(&next_node_id.as_str()) {
            return ChoiceResult::InvalidOption {
                current_node_id: session.current_node_id.to_string(),
                chosen_option: next_node_id.to_string(),
            };
        }

        let choice = self
            .get_current_node(session)
            .choices
            .iter()
            .find(|choice| choice.next_node_id == next_node_id)
            .unwrap()
            .clone();
        if let Some(command) = &choice.command {
            self.do_command(session, command);
        }

        session.current_node_id = next_node_id;

        ChoiceResult::Success
    }
}
