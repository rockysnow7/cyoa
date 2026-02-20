use std::fmt::Display;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::complete::{alphanumeric1, char, multispace0, multispace1},
    combinator::opt,
    multi::{many0, many1},
    sequence::{delimited, pair, preceded, separated_pair, terminated},
};

#[derive(Debug, Clone)]
pub enum FormatStringPart {
    Literal(String),
    Name(String),
}

#[derive(Debug, Clone)]
pub struct FormatString(pub Vec<FormatStringPart>);

#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i32),
    String(FormatString),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::String(s) => !s.0.is_empty(),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => f.write_str(if *b { "true" } else { "false" }),
            Value::Int(i) => f.write_str(&i.to_string()),
            Value::String(format_string) => {
                let s = format_string
                    .0
                    .iter()
                    .map(|part| match part {
                        FormatStringPart::Literal(s) => s.clone(),
                        FormatStringPart::Name(name) => format!("{{{name}}}"),
                    })
                    .collect::<String>();
                f.write_fmt(format_args!("\"{s}\""))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expression<'a> {
    Value(Value),
    Name(String),
    Equals {
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
    },
    NotEquals {
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
    },
    GreaterThan {
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
    },
    LessThan {
        left: &'a Expression<'a>,
        right: &'a Expression<'a>,
    },
}

impl Display for Expression<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(v) => f.write_str(v.to_string().as_str()),
            Self::Name(name) => f.write_str(name),
            Self::Equals { left, right } => f.write_fmt(format_args!("({left} = {right})")),
            Self::NotEquals { left, right } => f.write_fmt(format_args!("({left} != {right})")),
            Self::GreaterThan { left, right } => f.write_fmt(format_args!("({left} > {right})")),
            Self::LessThan { left, right } => f.write_fmt(format_args!("({left} < {right})")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command<'a> {
    Set { name: &'a str, value: Value },
}

impl Display for Command<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Set { name, value } => f.write_fmt(format_args!("SET {name} {value}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Choice<'a> {
    pub requirement: Option<Expression<'a>>,
    pub text: FormatString,
    pub next_node_id: String,
    pub command: Option<Command<'a>>,
}

#[derive(Debug, Clone)]
pub struct Node<'a> {
    pub display_text: FormatString,
    pub choices: Vec<Choice<'a>>,
}

fn parse_name(input: &str) -> IResult<&str, String> {
    many1(alt((alphanumeric1, tag("_"))))
        .map(|parts| parts.concat())
        .parse(input)
}

fn parse_id_definition(input: &str) -> IResult<&str, String> {
    preceded(pair(char('='), multispace0), parse_name).parse(input)
}

fn parse_primary_expression(input: &str) -> IResult<&str, Expression> {
    alt((
        parse_value.map(Expression::Value),
        parse_name.map(Expression::Name),
    ))
    .parse(input)
}

fn parse_expression(input: &str) -> IResult<&str, Expression> {
    let (input, left) = parse_primary_expression(input)?;
    if let Ok((input, (op, right))) = pair(
        delimited(
            multispace0,
            alt((tag("!="), tag("="), tag(">"), tag("<"))),
            multispace0,
        ),
        parse_primary_expression,
    )
    .parse(input)
    {
        let left = Box::leak(Box::new(left));
        let right = Box::leak(Box::new(right));
        Ok((
            input,
            match op {
                "=" => Expression::Equals { left, right },
                "!=" => Expression::NotEquals { left, right },
                ">" => Expression::GreaterThan { left, right },
                "<" => Expression::LessThan { left, right },
                _ => unreachable!(),
            },
        ))
    } else {
        Ok((input, left))
    }
}

fn parse_requirement(input: &str) -> IResult<&str, Expression> {
    delimited(
        (char('['), multispace0, tag("IF"), multispace0),
        parse_expression,
        (multispace0, char(']')),
    )
    .parse(input)
}

fn parse_command_set(input: &str) -> IResult<&str, Command> {
    (
        parse_name,
        delimited(multispace0, char('='), multispace0),
        parse_value,
    )
        .map(|(name, _, value)| Command::Set {
            name: Box::leak(name.into_boxed_str()),
            value,
        })
        .parse(input)
}

fn parse_command_inner(input: &str) -> IResult<&str, Command> {
    alt((parse_command_set,)).parse(input)
}

fn parse_command(input: &str) -> IResult<&str, Command> {
    delimited(
        (char('['), multispace0, tag("THEN"), multispace0),
        parse_command_inner,
        (multispace0, char(']')),
    )
    .parse(input)
}

fn parse_choice(input: &str) -> IResult<&str, Choice> {
    (
        opt(terminated(parse_requirement, multispace0)),
        separated_pair(
            parse_format_string,
            delimited(multispace0, tag("->"), multispace0),
            parse_name,
        ),
        opt(preceded(multispace0, parse_command)),
    )
        .map(|(requirement, (text, next_node_id), command)| Choice {
            requirement,
            text,
            next_node_id,
            command,
        })
        .parse(input)
}

fn parse_node_body(input: &str) -> IResult<&str, Node> {
    pair(
        preceded(multispace0, parse_format_string),
        many0(delimited(multispace0, parse_choice, multispace0)),
    )
    .map(|(display_text, choices)| Node {
        display_text,
        choices,
    })
    .parse(input)
}

fn parse_node_definition(input: &str) -> IResult<&str, (String, Node)> {
    pair(parse_id_definition, parse_node_body).parse(input)
}

fn parse_bool(input: &str) -> IResult<&str, Value> {
    alt((
        tag("true").map(|_| Value::Bool(true)),
        tag("false").map(|_| Value::Bool(false)),
    ))
    .parse(input)
}

fn parse_int(input: &str) -> IResult<&str, Value> {
    nom::character::complete::i32.map(Value::Int).parse(input)
}

fn parse_format_string_part_literal(input: &str) -> IResult<&str, FormatStringPart> {
    nom::bytes::complete::take_while1(|c: char| c != '"' && c != '{')
        .map(|s: &str| FormatStringPart::Literal(s.to_string()))
        .parse(input)
}

fn parse_format_string_part_name(input: &str) -> IResult<&str, FormatStringPart> {
    delimited(char('{'), parse_name, char('}'))
        .map(FormatStringPart::Name)
        .parse(input)
}

fn parse_format_string(input: &str) -> IResult<&str, FormatString> {
    delimited(
        char('"'),
        many0(alt((
            parse_format_string_part_literal,
            parse_format_string_part_name,
        ))),
        char('"'),
    )
    .map(FormatString)
    .parse(input)
}

fn parse_string(input: &str) -> IResult<&str, Value> {
    parse_format_string.map(Value::String).parse(input)
}

fn parse_value(input: &str) -> IResult<&str, Value> {
    alt((parse_bool, parse_int, parse_string)).parse(input)
}

fn parse_variable_definition(input: &str) -> IResult<&str, (String, Value)> {
    preceded(
        tag("SET"),
        pair(
            preceded(multispace1, parse_name),
            preceded(multispace1, parse_value),
        ),
    )
    .parse(input)
}

pub enum ProgramPart<'a> {
    NodeDefinition { id: String, node: Node<'a> },
    VariableDefinition { name: String, value: Value },
}

fn parse_program_part(input: &str) -> IResult<&str, ProgramPart> {
    alt((
        parse_node_definition.map(|(id, node)| ProgramPart::NodeDefinition { id, node }),
        parse_variable_definition
            .map(|(name, value)| ProgramPart::VariableDefinition { name, value }),
    ))
    .parse(input)
}

pub fn parse_program(input: &str) -> IResult<&str, Vec<ProgramPart>> {
    many0(delimited(multispace0, parse_program_part, multispace0)).parse(input)
}
