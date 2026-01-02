use super::*;

pub struct JSONDumper {
    ctx: Vec<Context>,
}

impl Dumper for JSONDumper {
    type Output = serde_json::Value;

    fn state(&mut self, state: &GameState) -> &mut Self {
        todo!()
    }

    fn context(&mut self, context: Context) -> &mut Self {
        todo!()
    }

    fn flush(&mut self) -> &mut Self {
        todo!()
    }

    fn dump(&mut self) -> Self::Output {
        todo!()
    }
}
