use std::ops::Range;

#[derive(Debug)]
pub struct Main<'s> {
    pub statements: Vec<Macro<'s>>,
}

#[derive(Debug)]
pub struct Name<'s> {
    pub span: Range<usize>,
    pub name: &'s str,
}

#[derive(Debug)]
pub struct Macro<'s> {
    pub name: Name<'s>,
    pub args: Vec<StrLit<'s>>,
}

#[derive(Debug)]
pub struct StrLit<'s> {
    pub span: Range<usize>,
    pub val: &'s str,
}
