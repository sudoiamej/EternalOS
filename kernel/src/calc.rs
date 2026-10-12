//! Arithmetic Expression Evaluator for EternalOS
//!
//! Evaluates integer expressions with operator precedence:
//! `*`, `/`, `%` > `+`, `-`
//! Supports parentheses `(`, `)`, unary minus, and zero-division guards.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Number(i64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
}

pub struct Tokenizer<'a> {
    chars: core::str::Chars<'a>,
    peeked: Option<char>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars(),
            peeked: None,
        }
    }

    fn next_char(&mut self) -> Option<char> {
        if let Some(ch) = self.peeked.take() {
            Some(ch)
        } else {
            self.chars.next()
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.peeked.is_none() {
            self.peeked = self.chars.next();
        }
        self.peeked
    }

    pub fn next_token(&mut self) -> Result<Option<Token>, &'static str> {
        loop {
            match self.next_char() {
                None => return Ok(None),
                Some(' ' | '\t' | '\r' | '\n') => continue,
                Some('+') => return Ok(Some(Token::Plus)),
                Some('-') => return Ok(Some(Token::Minus)),
                Some('*') => return Ok(Some(Token::Star)),
                Some('/') => return Ok(Some(Token::Slash)),
                Some('%') => return Ok(Some(Token::Percent)),
                Some('(') => return Ok(Some(Token::LParen)),
                Some(')') => return Ok(Some(Token::RParen)),
                Some(ch) if ch.is_ascii_digit() => {
                    let mut val = (ch as u8 - b'0') as i64;
                    while let Some(next_ch) = self.peek_char() {
                        if next_ch.is_ascii_digit() {
                            self.next_char();
                            val = val.checked_mul(10)
                                .and_then(|v| v.checked_add((next_ch as u8 - b'0') as i64))
                                .ok_or("Arithmetic overflow in integer literal")?;
                        } else {
                            break;
                        }
                    }
                    return Ok(Some(Token::Number(val)));
                }
                _ => return Err("Unexpected character in expression"),
            }
        }
    }
}

pub struct Parser<'a> {
    tokens: [Token; 64],
    len: usize,
    pos: usize,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Result<Self, &'static str> {
        let mut tokenizer = Tokenizer::new(input);
        let mut tokens = [Token::Plus; 64];
        let mut len = 0;

        while let Some(token) = tokenizer.next_token()? {
            if len >= tokens.len() {
                return Err("Expression too complex (token limit exceeded)");
            }
            tokens[len] = token;
            len += 1;
        }

        Ok(Self {
            tokens,
            len,
            pos: 0,
            _marker: core::marker::PhantomData,
        })
    }

    fn peek(&self) -> Option<Token> {
        if self.pos < self.len {
            Some(self.tokens[self.pos])
        } else {
            None
        }
    }

    fn consume(&mut self) -> Option<Token> {
        if self.pos < self.len {
            let tok = self.tokens[self.pos];
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    /// Parse addition and subtraction: Expr -> Term (('+' | '-') Term)*
    pub fn parse_expr(&mut self) -> Result<i64, &'static str> {
        let mut left = self.parse_term()?;

        while let Some(tok) = self.peek() {
            match tok {
                Token::Plus => {
                    self.consume();
                    let right = self.parse_term()?;
                    left = left.checked_add(right).ok_or("Arithmetic overflow during addition")?;
                }
                Token::Minus => {
                    self.consume();
                    let right = self.parse_term()?;
                    left = left.checked_sub(right).ok_or("Arithmetic overflow during subtraction")?;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// Parse multiplication, division, modulo: Term -> Factor (('*' | '/' | '%') Factor)*
    fn parse_term(&mut self) -> Result<i64, &'static str> {
        let mut left = self.parse_factor()?;

        while let Some(tok) = self.peek() {
            match tok {
                Token::Star => {
                    self.consume();
                    let right = self.parse_factor()?;
                    left = left.checked_mul(right).ok_or("Arithmetic overflow during multiplication")?;
                }
                Token::Slash => {
                    self.consume();
                    let right = self.parse_factor()?;
                    if right == 0 {
                        return Err("Division by zero");
                    }
                    left = left.checked_div(right).ok_or("Arithmetic overflow during division")?;
                }
                Token::Percent => {
                    self.consume();
                    let right = self.parse_factor()?;
                    if right == 0 {
                        return Err("Modulo by zero");
                    }
                    left = left.checked_rem(right).ok_or("Arithmetic overflow during modulo")?;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// Parse factors: numbers, parentheses, unary minus
    fn parse_factor(&mut self) -> Result<i64, &'static str> {
        match self.consume() {
            Some(Token::Number(n)) => Ok(n),
            Some(Token::Minus) => {
                let inner = self.parse_factor()?;
                inner.checked_neg().ok_or("Arithmetic overflow in unary negation")
            }
            Some(Token::Plus) => self.parse_factor(),
            Some(Token::LParen) => {
                let result = self.parse_expr()?;
                match self.consume() {
                    Some(Token::RParen) => Ok(result),
                    _ => Err("Missing closing parenthesis ')'"),
                }
            }
            _ => Err("Expected number or '('"),
        }
    }
}

/// Evaluates a math expression string and returns the computed result.
pub fn eval_expression(expr: &str) -> Result<i64, &'static str> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err("Empty expression");
    }

    let mut parser = Parser::new(trimmed)?;
    let result = parser.parse_expr()?;

    if parser.peek().is_some() {
        return Err("Unexpected trailing tokens after expression");
    }

    Ok(result)
}
