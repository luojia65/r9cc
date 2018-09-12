use token::{Token, TokenType};
use sema::Scope;
use util::size_of;

fn expect(ty: TokenType, t: &Token, pos: &mut usize) {
    if t.ty != ty {
        panic!(
            "{:?} ({:?}) expected, but got {:?} ({:?})",
            ty,
            ty,
            t.ty,
            t.ty
        );
    }
    *pos += 1;
}

fn consume(ty: TokenType, tokens: &Vec<Token>, pos: &mut usize) -> bool {
    let t = &tokens[*pos];
    if t.ty != ty {
        return false;
    }
    *pos += 1;
    return true;
}

fn get_type(t: &Token) -> Option<Type> {
    match t.ty {
        TokenType::Int => Some(Type::new(Ctype::Int)),
        TokenType::Char => Some(Type::new(Ctype::Char)),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum NodeType {
    Num(i32), // Number literal
    Str(String, usize), // String literal, (data, len)
    Ident(String), // Identifier
    Vardef(String, Option<Box<Node>>, Scope), // Variable definition, name = init
    Lvar(Scope), // Variable reference
    Gvar(String, String, usize), // Variable reference, (name, data, len)
    BinOp(TokenType, Box<Node>, Box<Node>), // left-hand, right-hand
    If(Box<Node>, Box<Node>, Option<Box<Node>>), // "if" ( cond ) then "else" els
    For(Box<Node>, Box<Node>, Box<Node>, Box<Node>), // "for" ( init; cond; inc ) body
    DoWhile(Box<Node>, Box<Node>), // do { body } while(cond)
    Addr(Box<Node>), // address-of operator("&"), expr
    Deref(Box<Node>), // pointer dereference ("*"), expr
    Logand(Box<Node>, Box<Node>), // left-hand, right-hand
    Logor(Box<Node>, Box<Node>), // left-hand, right-hand
    Return(Box<Node>), // "return", stmt
    Sizeof(Box<Node>), // "sizeof", expr
    Alignof(Box<Node>), // "_Alignof", expr
    Call(String, Vec<Node>), // Function call(name, args)
    // Function definition(name, args, body, stacksize)
    Func(String, Vec<Node>, Box<Node>, usize),
    CompStmt(Vec<Node>), // Compound statement
    ExprStmt(Box<Node>), // Expression statement
    StmtExpr(Box<Node>), // Statement expression (GNU extn.)
    Null,
}

#[derive(Debug, Clone)]
pub enum Ctype {
    Int,
    Char,
    Ptr(Box<Type>), // ptr of
    Ary(Box<Type>, usize), // ary of, len
}

impl Default for Ctype {
    fn default() -> Ctype {
        Ctype::Int
    }
}

#[derive(Debug, Clone)]
pub struct Type {
    pub ty: Ctype,
}

impl Default for Type {
    fn default() -> Type {
        Type { ty: Ctype::default() }
    }
}

impl Type {
    pub fn new(ty: Ctype) -> Self {
        Type { ty: ty }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub op: NodeType, // Node type
    pub ty: Box<Type>, // C type
}

impl Node {
    pub fn new(op: NodeType) -> Self {
        Self {
            op: op,
            ty: Box::new(Type::default()),
        }
    }

    pub fn new_int(val: i32) -> Self {
        Node::new(NodeType::Num(val))
    }

    pub fn new_binop(ty: TokenType, lhs: Node, rhs: Node) -> Self {
        Node::new(NodeType::BinOp(ty, Box::new(lhs), Box::new(rhs)))
    }
}

macro_rules! new_expr(
    ($i:path, $expr:expr) => (
        Node::new($i(Box::new($expr)))
    )
);

fn primary(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let t = &tokens[*pos];
    *pos += 1;
    match t.ty {
        TokenType::Num(val) => {
            let mut node = Node::new(NodeType::Num(val));
            node.ty = Box::new(Type::new(Ctype::Int));
            node
        }
        TokenType::Str(ref str, len) => {
            let mut node = Node::new(NodeType::Str(str.clone(), len));
            node.ty = Box::new(Type::new(
                Ctype::Ary(Box::new(Type::new(Ctype::Char)), str.len()),
            ));
            node
        }
        TokenType::Ident(ref name) => {
            if !consume(TokenType::LeftParen, tokens, pos) {
                return Node::new(NodeType::Ident(name.clone()));
            }

            let mut args = vec![];
            if consume(TokenType::RightParen, tokens, pos) {
                return Node::new(NodeType::Call(name.clone(), args));
            }

            args.push(assign(tokens, pos));
            while consume(TokenType::Colon, tokens, pos) {
                args.push(assign(tokens, pos));
            }
            expect(TokenType::RightParen, &tokens[*pos], pos);
            return Node::new(NodeType::Call(name.clone(), args));
        }
        TokenType::LeftParen => {
            if consume(TokenType::LeftBrace, tokens, pos) {
                let stmt = Box::new(compound_stmt(tokens, pos));
                expect(TokenType::RightParen, &tokens[*pos], pos);
                return Node::new(NodeType::StmtExpr(stmt));
            }
            let node = assign(tokens, pos);
            expect(TokenType::RightParen, &tokens[*pos], pos);
            node
        }
        _ => panic!("number expected, but got {}", t.input),
    }
}

fn postfix(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = primary(tokens, pos);
    while consume(TokenType::LeftBracket, tokens, pos) {
        lhs = new_expr!(
            NodeType::Deref,
            Node::new_binop(TokenType::Plus, lhs, assign(tokens, pos))
        );
        expect(TokenType::RightBracket, &tokens[*pos], pos);
    }
    lhs
}

fn unary(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    if consume(TokenType::Mul, tokens, pos) {
        return new_expr!(NodeType::Deref, mul(tokens, pos));
    }
    if consume(TokenType::And, tokens, pos) {
        return new_expr!(NodeType::Addr, mul(tokens, pos));
    }
    if consume(TokenType::Sizeof, tokens, pos) {
        return new_expr!(NodeType::Sizeof, unary(tokens, pos));
    }
    if consume(TokenType::Alignof, tokens, pos) {
        return new_expr!(NodeType::Alignof, unary(tokens, pos));
    }
    postfix(tokens, pos)
}

fn mul(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = unary(&tokens, pos);

    loop {
        if tokens.len() == *pos {
            return lhs;
        }

        let t = &tokens[*pos];
        if t.ty != TokenType::Mul && t.ty != TokenType::Div {
            return lhs;
        }
        *pos += 1;
        lhs = Node::new_binop(t.ty.clone(), lhs, unary(&tokens, pos));
    }
}

fn add(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = mul(&tokens, pos);

    loop {
        if tokens.len() == *pos {
            return lhs;
        }

        let t = &tokens[*pos];
        if t.ty != TokenType::Plus && t.ty != TokenType::Minus {
            return lhs;
        }
        *pos += 1;
        let rhs = mul(&tokens, pos);
        lhs = Node::new_binop(t.ty.clone(), lhs, rhs);
    }
}

fn rel(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = add(tokens, pos);
    loop {
        let t = &tokens[*pos];
        if t.ty == TokenType::LeftAngleBracket {
            *pos += 1;
            lhs = Node::new_binop(TokenType::LeftAngleBracket, lhs, add(tokens, pos));
            continue;
        }

        if t.ty == TokenType::RightAngleBracket {
            *pos += 1;
            lhs = Node::new_binop(TokenType::LeftAngleBracket, add(tokens, pos), lhs);
            continue;
        }

        return lhs;
    }
}

fn equality(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = rel(tokens, pos);
    loop {
        let t = &tokens[*pos];
        if t.ty == TokenType::EQ {
            *pos += 1;
            lhs = Node::new_binop(TokenType::EQ, lhs, rel(tokens, pos));
        }
        if t.ty == TokenType::NE {
            *pos += 1;
            lhs = Node::new_binop(TokenType::NE, lhs, rel(tokens, pos));
        }
        return lhs;
    }
}


fn logand(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = equality(tokens, pos);
    loop {
        if tokens[*pos].ty != TokenType::Logand {
            return lhs;
        }
        *pos += 1;
        lhs = Node::new(NodeType::Logand(
            Box::new(lhs),
            Box::new(equality(tokens, pos)),
        ));
    }
}

fn logor(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut lhs = logand(tokens, pos);
    loop {
        if tokens[*pos].ty != TokenType::Logor {
            return lhs;
        }
        *pos += 1;
        lhs = Node::new(NodeType::Logor(
            Box::new(lhs),
            Box::new(logand(tokens, pos)),
        ));
    }
}

fn assign(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let lhs = logor(tokens, pos);
    if consume(TokenType::Equal, tokens, pos) {
        return Node::new_binop(TokenType::Equal, lhs, logor(tokens, pos));
    }
    return lhs;
}

fn ctype(tokens: &Vec<Token>, pos: &mut usize) -> Type {
    let t = &tokens[*pos];
    if let Some(mut ty) = get_type(t) {
        *pos += 1;

        while consume(TokenType::Mul, tokens, pos) {
            ty = Type::new(Ctype::Ptr(Box::new(ty)));
        }
        ty
    } else {
        panic!("typename expected, but got {}", t.input);
    }
}

fn read_array(mut ty: Box<Type>, tokens: &Vec<Token>, pos: &mut usize) -> Box<Type> {
    let mut v: Vec<usize> = vec![];
    while consume(TokenType::LeftBracket, tokens, pos) {
        let len = primary(tokens, pos);
        if let NodeType::Num(n) = len.op {
            v.push(n as usize);
            expect(TokenType::RightBracket, &tokens[*pos], pos);
        } else {
            panic!("number expected");
        }
    }
    for val in v {
        ty = Box::new(Type::new(Ctype::Ary(ty, val)));
    }
    ty
}

fn decl(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    // Read the first half of type name (e.g. `int *`).
    let mut ty = Box::new(ctype(tokens, pos));

    let t = &tokens[*pos];
    // Read an identifier.
    if let TokenType::Ident(ref name) = t.ty {
        *pos += 1;
        let init: Option<Box<Node>>;

        // Read the second half of type name (e.g. `[3][5]`).
        ty = read_array(ty, tokens, pos);

        // Read an initializer.
        if consume(TokenType::Equal, tokens, pos) {
            init = Some(Box::new(assign(tokens, pos)));
        } else {
            init = None
        }
        expect(TokenType::Semicolon, &tokens[*pos], pos);
        let mut node = Node::new(NodeType::Vardef(name.clone(), init, Scope::Local(0)));
        node.ty = ty;
        node
    } else {
        panic!("variable name expected, but got {}", t.input);
    }
}

fn param(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let ty = Box::new(ctype(tokens, pos));
    let t = &tokens[*pos];
    if let TokenType::Ident(ref name) = t.ty {
        *pos += 1;
        let mut node = Node::new(NodeType::Vardef(name.clone(), None, Scope::Local(0)));
        node.ty = ty;
        node
    } else {
        panic!("parameter name expected, but got {}", t.input);
    }
}

fn expr_stmt(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let expr = assign(tokens, pos);
    let node = new_expr!(NodeType::ExprStmt, expr);
    expect(TokenType::Semicolon, &tokens[*pos], pos);
    node
}

fn stmt(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    match tokens[*pos].ty {
        TokenType::Int | TokenType::Char => return decl(tokens, pos),
        TokenType::If => {
            let mut els = None;
            *pos += 1;
            expect(TokenType::LeftParen, &tokens[*pos], pos);
            let cond = assign(&tokens, pos);
            expect(TokenType::RightParen, &tokens[*pos], pos);
            let then = stmt(&tokens, pos);
            if consume(TokenType::Else, tokens, pos) {
                els = Some(Box::new(stmt(&tokens, pos)));
            }
            Node::new(NodeType::If(Box::new(cond), Box::new(then), els))
        }
        TokenType::For => {
            *pos += 1;
            expect(TokenType::LeftParen, &tokens[*pos], pos);
            let init: Box<Node> = match get_type(&tokens[*pos]) {
                Some(_) => Box::new(decl(tokens, pos)),
                _ => Box::new(expr_stmt(tokens, pos)),
            };
            let cond = Box::new(assign(&tokens, pos));
            expect(TokenType::Semicolon, &tokens[*pos], pos);
            let inc = Box::new(new_expr!(NodeType::ExprStmt, assign(&tokens, pos)));
            expect(TokenType::RightParen, &tokens[*pos], pos);
            let body = Box::new(stmt(&tokens, pos));
            Node::new(NodeType::For(init, cond, inc, body))
        }
        TokenType::While => {
            *pos += 1;
            expect(TokenType::LeftParen, &tokens[*pos], pos);
            let init = Box::new(Node::new(NodeType::Null));
            let inc = Box::new(Node::new(NodeType::Null));
            let cond = Box::new(assign(&tokens, pos));
            expect(TokenType::RightParen, &tokens[*pos], pos);
            let body = Box::new(stmt(&tokens, pos));
            Node::new(NodeType::For(init, cond, inc, body))
        }
        TokenType::Do => {
            *pos += 1;
            let body = Box::new(stmt(tokens, pos));
            expect(TokenType::While, &tokens[*pos], pos);
            expect(TokenType::LeftParen, &tokens[*pos], pos);
            let cond = Box::new(assign(tokens, pos));
            expect(TokenType::RightParen, &tokens[*pos], pos);
            expect(TokenType::Semicolon, &tokens[*pos], pos);
            Node::new(NodeType::DoWhile(body, cond))
        }
        TokenType::Return => {
            *pos += 1;
            let expr = assign(&tokens, pos);
            expect(TokenType::Semicolon, &tokens[*pos], pos);
            Node::new(NodeType::Return(Box::new(expr)))
        }
        TokenType::LeftBrace => {
            *pos += 1;
            let mut stmts = vec![];
            while !consume(TokenType::RightBrace, tokens, pos) {
                stmts.push(stmt(&tokens, pos));
            }
            Node::new(NodeType::CompStmt(stmts))
        }
        TokenType::Semicolon => {
            *pos += 1;
            Node::new(NodeType::Null)
        }
        _ => {
            let expr = assign(&tokens, pos);
            let node = Node::new(NodeType::ExprStmt(Box::new(expr)));
            expect(TokenType::Semicolon, &tokens[*pos], pos);
            return node;
        }
    }
}

fn compound_stmt(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let mut stmts = vec![];

    while !consume(TokenType::RightBrace, tokens, pos) {
        stmts.push(stmt(tokens, pos));
    }
    Node::new(NodeType::CompStmt(stmts))
}

fn toplevel(tokens: &Vec<Token>, pos: &mut usize) -> Node {
    let is_extern = consume(TokenType::Extern, &tokens, pos);
    let ty = ctype(tokens, pos);
    let t = &tokens[*pos];
    let name: String;
    if let TokenType::Ident(ref name2) = t.ty {
        name = name2.clone();
    } else {
        panic!("function or variable name expected, but got {}", t.input);
    }
    *pos += 1;

    // Function
    if consume(TokenType::LeftParen, tokens, pos) {
        let mut args = vec![];
        if !consume(TokenType::RightParen, tokens, pos) {
            args.push(param(tokens, pos));
            while consume(TokenType::Colon, tokens, pos) {
                args.push(param(tokens, pos));
            }
            expect(TokenType::RightParen, &tokens[*pos], pos);
        }

        expect(TokenType::LeftBrace, &tokens[*pos], pos);
        let body = compound_stmt(tokens, pos);
        return Node::new(NodeType::Func(name, args, Box::new(body), 0));
    }

    // Global variable
    let ty = read_array(Box::new(ty), tokens, pos);
    let mut node;
    if is_extern {
        node = Node::new(NodeType::Vardef(
            name,
            None,
            Scope::Global(String::new(), 0, true),
        ));
    } else {
        node = Node::new(NodeType::Vardef(
            name,
            None,
            Scope::Global(String::new(), size_of(Box::new(&ty)), false),
        ));
    }
    node.ty = ty;
    expect(TokenType::Semicolon, &tokens[*pos], pos);
    node
}

/* e.g.
 function -> param
+---------+
int main() {     ; +-+                        int   []         2
  int ary[2];    ;   |               +->stmt->decl->read_array->primary
  ary[0]=1;      ;   | compound_stmt-+->stmt->...                ary
  return ary[0]; ;   |               +->stmt->assign->postfix-+->primary
}                ; +-+                  return        []      +->primary
                                                                 0
*/
pub fn parse(tokens: &Vec<Token>) -> Vec<Node> {
    let mut pos = 0;

    let mut v = vec![];
    while tokens.len() != pos {
        v.push(toplevel(tokens, &mut pos))
    }
    v
}
