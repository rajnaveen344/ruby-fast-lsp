//! RBS Lexer
//!
//! Tokenizes RBS source code into a stream of tokens.

use crate::types::ParseError;

/// Token kinds for RBS
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    Class,
    Module,
    Interface,
    End,
    Def,
    Include,
    Extend,
    Prepend,
    Attr_reader,
    Attr_writer,
    Attr_accessor,
    Public,
    Private,
    Protected,
    Alias,
    Type,
    SelfKeyword,
    Instance,
    Void,
    Untyped,
    Nil,
    Top,
    Bot,
    Bool,
    True,
    False,
    Singleton,
    Out,
    In,
    Unchecked,

    // Identifiers and literals
    Ident(String),
    UpperIdent(String),     // Starts with uppercase
    IvarIdent(String),      // @name
    CvarIdent(String),      // @@name
    GlobalIdent(String),    // $name
    InterfaceIdent(String), // _Name
    Symbol(String),         // :symbol
    StringLit(String),      // "string"
    IntLit(i64),            // 42

    // Operators and punctuation
    Arrow,       // ->
    FatArrow,    // =>
    DoubleColon, // ::
    Colon,       // :
    Comma,       // ,
    Dot,         // .
    Pipe,        // |
    Ampersand,   // &
    Question,    // ?
    Star,        // *
    DoubleStar,  // **
    Caret,       // ^
    LParen,      // (
    RParen,      // )
    LBracket,    // [
    LBrace,      // {
    RBracket,    // ]
    RBrace,      // }
    Lt,          // <
    Gt,          // >
    Eq,          // =
    Newline,
    Comment(String),
    Eof,
}

/// A token with position information
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self { kind, line, column }
    }
}

/// RBS Lexer
pub struct Lexer<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: usize,
    column: usize,
    current_pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            line: 1,
            column: 1,
            current_pos: 0,
        }
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_whitespace();

        let line = self.line;
        let column = self.column;

        match self.peek_char() {
            None => Ok(Token::new(TokenKind::Eof, line, column)),
            Some(c) => match c {
                '\n' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Newline, line, column))
                }
                '#' => {
                    let comment = self.read_comment();
                    Ok(Token::new(TokenKind::Comment(comment), line, column))
                }
                ':' => {
                    self.advance();
                    if self.peek_char() == Some(':') {
                        self.advance();
                        Ok(Token::new(TokenKind::DoubleColon, line, column))
                    } else if self
                        .peek_char()
                        .map(|c| c.is_alphabetic() || c == '_')
                        .unwrap_or(false)
                    {
                        let symbol = self.read_identifier();
                        Ok(Token::new(TokenKind::Symbol(symbol), line, column))
                    } else {
                        Ok(Token::new(TokenKind::Colon, line, column))
                    }
                }
                ',' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Comma, line, column))
                }
                '.' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Dot, line, column))
                }
                '|' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Pipe, line, column))
                }
                '&' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Ampersand, line, column))
                }
                '?' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Question, line, column))
                }
                '*' => {
                    self.advance();
                    if self.peek_char() == Some('*') {
                        self.advance();
                        Ok(Token::new(TokenKind::DoubleStar, line, column))
                    } else {
                        Ok(Token::new(TokenKind::Star, line, column))
                    }
                }
                '^' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Caret, line, column))
                }
                '(' => {
                    self.advance();
                    Ok(Token::new(TokenKind::LParen, line, column))
                }
                ')' => {
                    self.advance();
                    Ok(Token::new(TokenKind::RParen, line, column))
                }
                '[' => {
                    self.advance();
                    Ok(Token::new(TokenKind::LBracket, line, column))
                }
                ']' => {
                    self.advance();
                    Ok(Token::new(TokenKind::RBracket, line, column))
                }
                '{' => {
                    self.advance();
                    Ok(Token::new(TokenKind::LBrace, line, column))
                }
                '}' => {
                    self.advance();
                    Ok(Token::new(TokenKind::RBrace, line, column))
                }
                '<' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Lt, line, column))
                }
                '>' => {
                    self.advance();
                    Ok(Token::new(TokenKind::Gt, line, column))
                }
                '=' => {
                    self.advance();
                    if self.peek_char() == Some('>') {
                        self.advance();
                        Ok(Token::new(TokenKind::FatArrow, line, column))
                    } else {
                        Ok(Token::new(TokenKind::Eq, line, column))
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek_char() == Some('>') {
                        self.advance();
                        Ok(Token::new(TokenKind::Arrow, line, column))
                    } else if self
                        .peek_char()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                    {
                        let num = self.read_number(true);
                        Ok(Token::new(TokenKind::IntLit(num), line, column))
                    } else {
                        Err(ParseError::new("Unexpected '-'", line, column))
                    }
                }
                '"' => {
                    let s = self.read_string()?;
                    Ok(Token::new(TokenKind::StringLit(s), line, column))
                }
                '@' => {
                    self.advance();
                    if self.peek_char() == Some('@') {
                        self.advance();
                        let name = self.read_identifier();
                        Ok(Token::new(
                            TokenKind::CvarIdent(format!("@@{}", name)),
                            line,
                            column,
                        ))
                    } else {
                        let name = self.read_identifier();
                        Ok(Token::new(
                            TokenKind::IvarIdent(format!("@{}", name)),
                            line,
                            column,
                        ))
                    }
                }
                '$' => {
                    self.advance();
                    let name = self.read_identifier();
                    Ok(Token::new(
                        TokenKind::GlobalIdent(format!("${}", name)),
                        line,
                        column,
                    ))
                }
                '_' => {
                    let ident = self.read_identifier();
                    if ident
                        .chars()
                        .nth(1)
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        Ok(Token::new(TokenKind::InterfaceIdent(ident), line, column))
                    } else {
                        Ok(Token::new(self.keyword_or_ident(&ident), line, column))
                    }
                }
                c if c.is_ascii_digit() => {
                    let num = self.read_number(false);
                    Ok(Token::new(TokenKind::IntLit(num), line, column))
                }
                c if c.is_alphabetic() || c == '_' => {
                    let ident = self.read_identifier();
                    let kind = self.keyword_or_ident(&ident);
                    Ok(Token::new(kind, line, column))
                }
                c => Err(ParseError::new(
                    format!("Unexpected character: '{}'", c),
                    line,
                    column,
                )),
            },
        }
    }

    /// Tokenize the entire source
    pub fn tokenize(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn advance(&mut self) -> Option<char> {
        if let Some((pos, c)) = self.chars.next() {
            self.current_pos = pos;
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(c)
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' || c == '!' || c == '?' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    fn read_number(&mut self, negative: bool) -> i64 {
        let mut num_str = String::new();
        if negative {
            num_str.push('-');
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '_' {
                if c != '_' {
                    num_str.push(c);
                }
                self.advance();
            } else {
                break;
            }
        }
        num_str.parse().unwrap_or(0)
    }

    fn read_string(&mut self) -> Result<String, ParseError> {
        let line = self.line;
        let column = self.column;
        self.advance(); // consume opening "
        let mut s = String::new();
        loop {
            match self.peek_char() {
                None => return Err(ParseError::new("Unterminated string", line, column)),
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some('"') => s.push('"'),
                        Some(c) => s.push(c),
                        None => {
                            return Err(ParseError::new("Unterminated string escape", line, column))
                        }
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.advance();
                }
            }
        }
        Ok(s)
    }

    fn read_comment(&mut self) -> String {
        let mut comment = String::new();
        self.advance(); // consume #
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            comment.push(c);
            self.advance();
        }
        comment
    }

    fn keyword_or_ident(&self, ident: &str) -> TokenKind {
        match ident {
            "class" => TokenKind::Class,
            "module" => TokenKind::Module,
            "interface" => TokenKind::Interface,
            "end" => TokenKind::End,
            "def" => TokenKind::Def,
            "include" => TokenKind::Include,
            "extend" => TokenKind::Extend,
            "prepend" => TokenKind::Prepend,
            "attr_reader" => TokenKind::Attr_reader,
            "attr_writer" => TokenKind::Attr_writer,
            "attr_accessor" => TokenKind::Attr_accessor,
            "public" => TokenKind::Public,
            "private" => TokenKind::Private,
            "protected" => TokenKind::Protected,
            "alias" => TokenKind::Alias,
            "type" => TokenKind::Type,
            "self" => TokenKind::SelfKeyword,
            "instance" => TokenKind::Instance,
            "void" => TokenKind::Void,
            "untyped" => TokenKind::Untyped,
            "nil" => TokenKind::Nil,
            "top" => TokenKind::Top,
            "bot" => TokenKind::Bot,
            "bool" => TokenKind::Bool,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "singleton" => TokenKind::Singleton,
            "out" => TokenKind::Out,
            "in" => TokenKind::In,
            "unchecked" => TokenKind::Unchecked,
            _ => {
                if ident
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                {
                    TokenKind::UpperIdent(ident.to_string())
                } else {
                    TokenKind::Ident(ident.to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_simple_class() {
        let source = "class String\nend";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::Class);
        assert_eq!(tokens[1].kind, TokenKind::UpperIdent("String".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Newline);
        assert_eq!(tokens[3].kind, TokenKind::End);
        assert_eq!(tokens[4].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lexer_method_signature() {
        let source = "def length: () -> Integer";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::Def);
        assert_eq!(tokens[1].kind, TokenKind::Ident("length".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Colon);
        assert_eq!(tokens[3].kind, TokenKind::LParen);
        assert_eq!(tokens[4].kind, TokenKind::RParen);
        assert_eq!(tokens[5].kind, TokenKind::Arrow);
        assert_eq!(tokens[6].kind, TokenKind::UpperIdent("Integer".to_string()));
    }

    #[test]
    fn test_lexer_generic_type() {
        let source = "Array[String]";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::UpperIdent("Array".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::LBracket);
        assert_eq!(tokens[2].kind, TokenKind::UpperIdent("String".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::RBracket);
    }

    #[test]
    fn test_lexer_union_type() {
        let source = "String | Integer | nil";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::UpperIdent("String".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Pipe);
        assert_eq!(tokens[2].kind, TokenKind::UpperIdent("Integer".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::Pipe);
        assert_eq!(tokens[4].kind, TokenKind::Nil);
    }

    #[test]
    fn test_lexer_optional_type() {
        let source = "String?";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::UpperIdent("String".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Question);
    }

    #[test]
    fn test_lexer_double_colon() {
        let source = "::Integer";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::DoubleColon);
        assert_eq!(tokens[1].kind, TokenKind::UpperIdent("Integer".to_string()));
    }

    #[test]
    fn test_lexer_symbol() {
        let source = ":name";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::Symbol("name".to_string()));
    }

    #[test]
    fn test_lexer_instance_variable() {
        let source = "@name: String";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::IvarIdent("@name".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Colon);
        assert_eq!(tokens[2].kind, TokenKind::UpperIdent("String".to_string()));
    }

    #[test]
    fn test_lexer_comment() {
        let source = "# This is a comment\nclass Foo";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert!(matches!(tokens[0].kind, TokenKind::Comment(_)));
        assert_eq!(tokens[1].kind, TokenKind::Newline);
        assert_eq!(tokens[2].kind, TokenKind::Class);
    }

    #[test]
    fn test_lexer_proc_type() {
        let source = "^(String) -> Integer";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].kind, TokenKind::Caret);
        assert_eq!(tokens[1].kind, TokenKind::LParen);
        assert_eq!(tokens[2].kind, TokenKind::UpperIdent("String".to_string()));
        assert_eq!(tokens[3].kind, TokenKind::RParen);
        assert_eq!(tokens[4].kind, TokenKind::Arrow);
        assert_eq!(tokens[5].kind, TokenKind::UpperIdent("Integer".to_string()));
    }
}
