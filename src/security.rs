use regex::Regex;
use once_cell::sync::Lazy;

/// SQL injection protection patterns
static SQL_INJECTION_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Single quote injections
        Regex::new(r"'.*?(\s+(OR|AND)\s+|;|--|#|/\*|\*/|UNION|SELECT|INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|EXEC|SCRIPT)").unwrap(),
        // Double quote injections
        Regex::new(r#"".*?(\s+(OR|AND)\s+|;|--|#|/\*|\*/|UNION|SELECT|INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|EXEC|SCRIPT)"#).unwrap(),
        // Backtick injections
        Regex::new(r"`.*?(\s+(OR|AND)\s+|;|--|#|/\*|\*/|UNION|SELECT|INSERT|UPDATE|DELETE|DROP|CREATE|ALTER|EXEC|SCRIPT)").unwrap(),
        // Common SQL keywords without quotes
        Regex::new(r"(?i)\b(UNION\s+SELECT|INSERT\s+INTO|UPDATE\s+SET|DELETE\s+FROM|DROP\s+(TABLE|DATABASE)|CREATE\s+(TABLE|DATABASE)|ALTER\s+TABLE|EXEC\s*\(|xp_cmdshell|sp_executesql)\b").unwrap(),
        // Comment sequences
        Regex::new(r"(--|#|/\*|\*/)").unwrap(),
        // Semicolon for statement termination
        Regex::new(r";").unwrap(),
        // Hex encoding attempts
        Regex::new(r"0x[0-9a-fA-F]+").unwrap(),
    ]
});

/// XSS protection patterns
static XSS_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Script tags
        Regex::new(r"(?i)<\s*script[^>]*>.*?</\s*script\s*>").unwrap(),
        // Event handlers
        Regex::new(r"(?i)\s*on\w+\s*=").unwrap(),
        // JavaScript protocol
        Regex::new(r"(?i)javascript\s*:").unwrap(),
        // Data URI with script
        Regex::new(r"(?i)data\s*:.*script").unwrap(),
        // HTML tags that can execute code
        Regex::new(r"(?i)<\s*(iframe|embed|object|applet|form|input|button|select|textarea|img|svg|video|audio|source|track|script|style|link|meta|base)[^>]*>").unwrap(),
        // VBScript
        Regex::new(r"(?i)vbscript\s*:").unwrap(),
        // Expression evaluation
        Regex::new(r"(?i)expression\s*\(").unwrap(),
    ]
});

/// Command injection protection patterns
static COMMAND_INJECTION_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Shell metacharacters
        Regex::new(r"[;&|`$(){}[\]<>]").unwrap(),
        // Command substitution
        Regex::new(r"\$\([^)]+\)").unwrap(),
        // Backtick command substitution
        Regex::new(r"`[^`]+`").unwrap(),
        // Newline characters
        Regex::new(r"[\r\n]").unwrap(),
        // Common dangerous commands
        Regex::new(r"(?i)\b(rm|del|format|shutdown|reboot|kill|pkill|dd|mkfs|wget|curl|nc|netcat|python|perl|ruby|php|bash|sh|cmd|powershell)\b").unwrap(),
    ]
});

/// Path traversal protection patterns
static PATH_TRAVERSAL_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Directory traversal
        Regex::new(r"\.\./?").unwrap(),
        // Encoded traversal
        Regex::new(r"%2e%2e|%252e%252e").unwrap(),
        // Unicode traversal
        Regex::new(r"\\u002e\\u002e").unwrap(),
        // Absolute paths
        Regex::new(r"^/|^[a-zA-Z]:").unwrap(),
        // UNC paths
        Regex::new(r"^\\\\").unwrap(),
    ]
});

/// Input sanitization functions
pub struct SecurityValidator;

impl SecurityValidator {
    /// Check for SQL injection attempts
    pub fn has_sql_injection(input: &str) -> bool {
        let upper_input = input.to_uppercase();
        SQL_INJECTION_PATTERNS.iter().any(|pattern| pattern.is_match(&upper_input))
    }
    
    /// Check for XSS attempts
    pub fn has_xss(input: &str) -> bool {
        XSS_PATTERNS.iter().any(|pattern| pattern.is_match(input))
    }
    
    /// Check for command injection attempts
    pub fn has_command_injection(input: &str) -> bool {
        COMMAND_INJECTION_PATTERNS.iter().any(|pattern| pattern.is_match(input))
    }
    
    /// Check for path traversal attempts
    pub fn has_path_traversal(input: &str) -> bool {
        PATH_TRAVERSAL_PATTERNS.iter().any(|pattern| pattern.is_match(input))
    }
    
    /// Sanitize input for safe use (removes dangerous characters)
    pub fn sanitize_input(input: &str) -> String {
        // Remove all potentially dangerous characters
        let mut sanitized = input.to_string();
        
        // Remove SQL dangerous chars
        sanitized = sanitized.replace(&['\'', '"', ';', '-', '#', '/', '*'][..], "");
        
        // Remove shell dangerous chars
        sanitized = sanitized.replace(&['&', '|', '`', '$', '(', ')', '{', '}', '[', ']', '<', '>'][..], "");
        
        // Remove path traversal
        sanitized = sanitized.replace("..", "");
        
        // Remove newlines and control characters
        sanitized = sanitized.chars()
            .filter(|c| !c.is_control() && *c != '\r' && *c != '\n')
            .collect();
        
        sanitized
    }
    
    /// Validate that input is safe for database queries
    pub fn validate_database_input(input: &str) -> Result<(), &'static str> {
        if Self::has_sql_injection(input) {
            return Err("SQL injection attempt detected");
        }
        Ok(())
    }
    
    /// Validate that input is safe for HTML output
    pub fn validate_html_input(input: &str) -> Result<(), &'static str> {
        if Self::has_xss(input) {
            return Err("XSS attempt detected");
        }
        Ok(())
    }
    
    /// Validate that input is safe for system commands
    pub fn validate_command_input(input: &str) -> Result<(), &'static str> {
        if Self::has_command_injection(input) {
            return Err("Command injection attempt detected");
        }
        Ok(())
    }
    
    /// Validate that input is safe for file paths
    pub fn validate_path_input(input: &str) -> Result<(), &'static str> {
        if Self::has_path_traversal(input) {
            return Err("Path traversal attempt detected");
        }
        Ok(())
    }
    
    /// Comprehensive input validation
    pub fn validate_all(input: &str) -> Result<(), &'static str> {
        Self::validate_database_input(input)?;
        Self::validate_html_input(input)?;
        Self::validate_command_input(input)?;
        Self::validate_path_input(input)?;
        Ok(())
    }
}

/// Escape string for safe SQL usage (as backup to parameterized queries)
pub fn escape_sql_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\0', "\\0")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\x1a', "\\Z")
}

/// Escape string for safe HTML output
pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
        .replace('/', "&#x2F;")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sql_injection_detection() {
        assert!(SecurityValidator::has_sql_injection("'; DROP TABLE orders; --"));
        assert!(SecurityValidator::has_sql_injection("' OR '1'='1"));
        assert!(SecurityValidator::has_sql_injection("admin'--"));
        assert!(SecurityValidator::has_sql_injection("' UNION SELECT * FROM users --"));
        assert!(!SecurityValidator::has_sql_injection("normal_input"));
    }
    
    #[test]
    fn test_xss_detection() {
        assert!(SecurityValidator::has_xss("<script>alert('XSS')</script>"));
        assert!(SecurityValidator::has_xss("<img src=x onerror=alert('XSS')>"));
        assert!(SecurityValidator::has_xss("javascript:alert('XSS')"));
        assert!(!SecurityValidator::has_xss("normal text"));
    }
    
    #[test]
    fn test_command_injection_detection() {
        assert!(SecurityValidator::has_command_injection("; ls -la"));
        assert!(SecurityValidator::has_command_injection("| cat /etc/passwd"));
        assert!(SecurityValidator::has_command_injection("`whoami`"));
        assert!(!SecurityValidator::has_command_injection("normal command"));
    }
    
    #[test]
    fn test_path_traversal_detection() {
        assert!(SecurityValidator::has_path_traversal("../../../etc/passwd"));
        assert!(SecurityValidator::has_path_traversal("..\\..\\windows\\system32"));
        assert!(SecurityValidator::has_path_traversal("%2e%2e%2fetc%2fpasswd"));
        assert!(!SecurityValidator::has_path_traversal("normal/path/file.txt"));
    }
    
    #[test]
    fn test_input_sanitization() {
        assert_eq!(SecurityValidator::sanitize_input("'; DROP TABLE--"), " DROP TABLE");
        assert_eq!(SecurityValidator::sanitize_input("normal_input"), "normal_input");
        assert_eq!(SecurityValidator::sanitize_input("test$(rm -rf /)"), "testrm -rf ");
    }
}