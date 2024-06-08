use std::collections::HashMap;
use std::{any::Any, fmt};

use crate::dsl_schemas::{
    Action, ComparisonOperator, Expression, FunctionCall, Param, StepBody, Workflow, WorkflowValue,
};

/*
TODOs:
- we want to return all the steps that were executed, not just the final registers (this is for step_history)
- we want to return specific errors
- logging + feedback for the user + feedback for workflow devs
- let's start with basic fn like inference
- we can have another fn that's a more custom inference
 */

#[derive(Debug)]
pub enum WorkflowError {
    FunctionError(String),
    EvaluationError(String),
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowError::FunctionError(msg) => write!(f, "Function error: {}", msg),
            WorkflowError::EvaluationError(msg) => write!(f, "Evaluation error: {}", msg),
        }
    }
}

impl std::error::Error for WorkflowError {}

pub type FunctionMap<'a> =
    HashMap<String, Box<dyn Fn(Vec<Box<dyn Any>>) -> Result<Box<dyn Any>, WorkflowError> + Send + Sync + 'a>>;

pub struct WorkflowEngine<'a> {
    functions: &'a FunctionMap<'a>,
}

pub struct StepExecutor<'a> {
    engine: &'a WorkflowEngine<'a>,
    workflow: &'a Workflow,
    pub current_step: usize,
    pub registers: HashMap<String, i32>,
}

impl<'a> WorkflowEngine<'a> {
    pub fn new(functions: &'a FunctionMap<'a>) -> Self {
        WorkflowEngine { functions }
    }

    pub fn execute_workflow(&self, workflow: &Workflow) -> Result<HashMap<String, i32>, WorkflowError> {
        let mut registers = HashMap::new();
        for step in &workflow.steps {
            for body in &step.body {
                self.execute_step_body(body, &mut registers)?;
            }
        }
        Ok(registers)
    }

    pub fn execute_step_body(
        &self,
        step_body: &StepBody,
        registers: &mut HashMap<String, i32>,
    ) -> Result<(), WorkflowError> {
        match step_body {
            StepBody::Action(action) => self.execute_action(action, registers),
            StepBody::Condition { condition, body } => {
                if self.evaluate_condition(condition, registers) {
                    self.execute_step_body(body, registers)?;
                }
                Ok(())
            }
            StepBody::ForLoop { var, in_expr, action } => {
                if let Expression::Range { start, end } = in_expr {
                    let start = self.evaluate_param(start.as_ref(), registers);
                    let end = self.evaluate_param(end.as_ref(), registers);
                    for i in start..=end {
                        registers.insert(var.clone(), i);
                        self.execute_step_body(action, registers)?;
                    }
                }
                Ok(())
            }
            StepBody::RegisterOperation { register, value } => {
                println!("Setting register {} to {:?}", register, value);
                let value = self.evaluate_workflow_value(value, registers);
                println!("Value: {}", value);
                registers.insert(register.clone(), value);
                Ok(())
            }
            StepBody::Composite(bodies) => {
                for body in bodies {
                    self.execute_step_body(body, registers)?;
                }
                Ok(())
            }
        }
    }

    pub fn execute_action(&self, action: &Action, registers: &mut HashMap<String, i32>) -> Result<(), WorkflowError> {
        println!("Executing action: {:?}", action);
        match action {
            Action::ExternalFnCall(FunctionCall { name, args }) => {
                if let Some(func) = self.functions.get(name) {
                    let arg_values = args
                        .iter()
                        .map(|arg| Box::new(self.evaluate_param(arg, registers)) as Box<dyn Any>)
                        .collect();
                    let result = func(arg_values)?;
                    if let Ok(result) = result.downcast::<i32>() {
                        if let Some(Param::Identifier(register_name)) = args.first() {
                            registers.insert(register_name.clone(), *result);
                        }
                    }
                }
                Ok(())
            }
            _ => Err(WorkflowError::FunctionError(format!("Unhandled action: {:?}", action))),
        }
    }

    pub fn evaluate_condition(&self, expression: &Expression, registers: &HashMap<String, i32>) -> bool {
        match expression {
            Expression::Binary { left, operator, right } => {
                let left_val = self.evaluate_param(left, registers);
                let right_val = self.evaluate_param(right, registers);
                match operator {
                    ComparisonOperator::Less => left_val < right_val,
                    ComparisonOperator::Greater => left_val > right_val,
                    ComparisonOperator::Equal => left_val == right_val,
                    ComparisonOperator::NotEqual => left_val != right_val,
                    ComparisonOperator::LessEqual => left_val <= right_val,
                    ComparisonOperator::GreaterEqual => left_val >= right_val,
                }
            }
            _ => false,
        }
    }

    pub fn evaluate_param(&self, param: &Param, registers: &HashMap<String, i32>) -> i32 {
        match param {
            Param::Number(n) => *n as i32,
            Param::Identifier(id) | Param::Register(id) => registers.get(id).copied().unwrap_or_else(|| {
                eprintln!(
                    "Warning: Identifier/Register '{}' not found in registers, defaulting to 0",
                    id
                );
                0
            }),
            _ => {
                eprintln!("Warning: Unsupported parameter type, defaulting to 0");
                0
            }
        }
    }

    pub fn evaluate_workflow_value(&self, value: &WorkflowValue, registers: &HashMap<String, i32>) -> i32 {
        match value {
            WorkflowValue::Number(n) => *n as i32,
            WorkflowValue::Identifier(id) => *registers.get(id).unwrap_or(&0),
            WorkflowValue::FunctionCall(FunctionCall { name, args }) => {
                if let Some(func) = self.functions.get(name) {
                    let arg_values = args
                        .iter()
                        .map(|arg| Box::new(self.evaluate_param(arg, registers)) as Box<dyn Any>)
                        .collect();
                    let result = func(arg_values);
                    match result {
                        Ok(result) => {
                            if let Ok(result) = result.downcast::<i32>() {
                                *result
                            } else {
                                eprintln!("Function call to '{}' did not return an i32.", name);
                                0
                            }
                        }
                        Err(err) => {
                            eprintln!("Error executing function '{}': {}", name, err);
                            0
                        }
                    }
                } else {
                    eprintln!("Function '{}' not found.", name);
                    0
                }
            }
            _ => {
                eprintln!("Unsupported workflow value type {:?}, defaulting to 0", value);
                0
            }
        }
    }

    pub fn iter(&'a self, workflow: &'a Workflow) -> StepExecutor<'a> {
        StepExecutor {
            engine: self,
            workflow,
            current_step: 0,
            registers: HashMap::new(),
        }
    }
}

impl<'a> Iterator for StepExecutor<'a> {
    type Item = Result<HashMap<String, i32>, WorkflowError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_step < self.workflow.steps.len() {
            let step = &self.workflow.steps[self.current_step];
            for body in &step.body {
                if let Err(e) = self.engine.execute_step_body(body, &mut self.registers) {
                    return Some(Err(e));
                }
            }
            self.current_step += 1;
            Some(Ok(self.registers.clone()))
        } else {
            None
        }
    }
}
