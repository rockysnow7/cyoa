mod parser;

use parser::{
    Command, Expression, FormatString, FormatStringPart, Node, ProgramPart, Value, parse_program,
};
use serde::Serialize;
use std::{collections::HashMap, fmt::Display};

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

pub struct Engine<'a> {
    variables: HashMap<String, Value>,
    all_nodes: HashMap<String, Node<'a>>,
    current_node_id: String,
}

impl<'a> Engine<'a> {
    pub fn new() -> Self {
        Engine {
            variables: HashMap::new(),
            all_nodes: HashMap::new(),
            current_node_id: "START".to_string(),
        }
    }

    fn bad_names_in_string(&self, s: &FormatString) -> Vec<String> {
        let mut bad_names = Vec::new();
        for part in &s.0 {
            if let FormatStringPart::Name(name) = part {
                if !self.variables.contains_key(name) {
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
                if !self.variables.contains_key(name) {
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
            Expression::Name(name) => self.variables.contains_key(name),
            Expression::Equals { left, right } | Expression::NotEquals { left, right } => {
                self.expression_is_valid(left) && self.expression_is_valid(right)
            }
            Expression::GreaterThan { left, right } | Expression::LessThan { left, right } => {
                let left_is_int = if let Expression::Value(Value::Int(_)) = left {
                    true
                } else if let Expression::Name(name) = left {
                    matches!(self.variables.get(name), Some(Value::Int(_)))
                } else {
                    false
                };
                let right_is_int = if let Expression::Value(Value::Int(_)) = right {
                    true
                } else if let Expression::Name(name) = right {
                    matches!(self.variables.get(name), Some(Value::Int(_)))
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
                if !self.variables.contains_key(*name) {
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
                self.variables.contains_key(*name)
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
                engine.variables.insert(name.to_string(), value.clone());
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

    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::Int(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => self.evaluate_string(s),
        }
    }

    fn evaluate_string(&self, input: &FormatString) -> String {
        let mut result = String::new();
        for part in &input.0 {
            match part {
                FormatStringPart::Literal(s) => result.push_str(s),
                FormatStringPart::Name(name) => {
                    let var_value = self
                        .variables
                        .get(name)
                        .map(|v| self.value_to_string(v))
                        .unwrap();
                    result.push_str(var_value.as_str());
                }
            }
        }

        result
    }

    fn values_are_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(l), Value::Int(r)) => l == r,
            (Value::Bool(l), Value::Bool(r)) => l == r,
            (Value::String(l), Value::String(r)) => {
                self.evaluate_string(l) == self.evaluate_string(r)
            }
            _ => false,
        }
    }

    fn evaluate_expression(&self, input: &Expression) -> Value {
        match input {
            Expression::Value(v) => v.clone(),
            Expression::Name(name) => self.variables.get(name).unwrap().clone(),
            Expression::Equals { left, right } => {
                let left_val = self.evaluate_expression(left);
                let right_val = self.evaluate_expression(right);
                Value::Bool(self.values_are_equal(&left_val, &right_val))
            }
            Expression::NotEquals { left, right } => {
                let left_val = self.evaluate_expression(left);
                let right_val = self.evaluate_expression(right);
                Value::Bool(!self.values_are_equal(&left_val, &right_val))
            }
            Expression::GreaterThan { left, right } => {
                let left_val = self.evaluate_expression(left);
                let right_val = self.evaluate_expression(right);
                match (left_val, right_val) {
                    (Value::Int(l), Value::Int(r)) => Value::Bool(l > r),
                    _ => panic!("GreaterThan operator can only be applied to integers"),
                }
            }
            Expression::LessThan { left, right } => {
                let left_val = self.evaluate_expression(left);
                let right_val = self.evaluate_expression(right);
                match (left_val, right_val) {
                    (Value::Int(l), Value::Int(r)) => Value::Bool(l < r),
                    _ => panic!("LessThan operator can only be applied to integers"),
                }
            }
        }
    }

    fn get_current_node(&self) -> &Node<'a> {
        self.all_nodes.get(self.current_node_id.as_str()).unwrap()
    }

    pub fn get_valid_options_ids(&'a self) -> Vec<&'a str> {
        self.get_current_node()
            .choices
            .iter()
            .map(|choice| choice.next_node_id.as_str())
            .collect()
    }

    pub fn get_current_node_view(&self) -> CurrentNodeView {
        let current_node = self.get_current_node();

        let display_text = self.evaluate_string(&current_node.display_text);
        let choices = current_node
            .choices
            .iter()
            .filter_map(|choice| {
                if let Some(req) = &choice.requirement {
                    if !self.evaluate_expression(req).is_truthy() {
                        return None;
                    }
                }

                Some(ChoiceView {
                    id: choice.next_node_id.to_string(),
                    display_text: self.evaluate_string(&choice.text),
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

    fn do_command(&mut self, command: &Command) {
        match command {
            Command::Set { name, value } => {
                let var = self.variables.get_mut(*name).unwrap();
                *var = value.clone();
            }
        }
    }

    pub fn choose_option(&mut self, next_node_id: String) -> ChoiceResult {
        let valid_options = self.get_valid_options_ids();
        if !valid_options.contains(&next_node_id.as_str()) {
            return ChoiceResult::InvalidOption {
                current_node_id: self.current_node_id.to_string(),
                chosen_option: next_node_id.to_string(),
            };
        }

        let choice = self
            .get_current_node()
            .choices
            .iter()
            .find(|choice| choice.next_node_id == next_node_id)
            .unwrap()
            .clone();
        if let Some(command) = &choice.command {
            self.do_command(command);
        }

        self.current_node_id = next_node_id;

        ChoiceResult::Success
    }
}
