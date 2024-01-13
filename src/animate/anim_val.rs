use std::{any::Any, rc::Rc};

use floem_peniko::Color;

#[derive(Debug, Clone)]
pub enum AnimValue {
    Float(f64),
    Color(Color),
    Prop(Rc<dyn Any>),
}

impl AnimValue {
    pub fn get_f32(self) -> f32 {
        self.get_f64() as f32
    }

    pub fn get_f64(self) -> f64 {
        match self {
            AnimValue::Float(v) => v,
            AnimValue::Color(_) => panic!(),
            AnimValue::Prop(prop) => *prop.downcast_ref::<f64>().unwrap(),
        }
    }

    pub fn get_color(self) -> Color {
        match self {
            AnimValue::Color(c) => c,
            AnimValue::Float(_) => panic!(),
            AnimValue::Prop(prop) => *prop.downcast_ref::<Color>().unwrap(),
        }
    }

    pub fn get_any(self) -> Rc<dyn Any> {
        match self {
            AnimValue::Color(_) => panic!(),
            AnimValue::Float(_) => panic!(),
            AnimValue::Prop(prop) => prop.clone(),
        }
    }
}
