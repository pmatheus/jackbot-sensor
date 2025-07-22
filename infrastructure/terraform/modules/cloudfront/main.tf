# CloudFront Distribution Module for Global Performance
# Provides global edge locations and intelligent routing

variable "project_name" {
  description = "Project name for resource naming"
  type        = string
}

variable "environment" {
  description = "Environment name"
  type        = string
}

variable "regional_origins" {
  description = "Regional origin configurations"
  type = map(object({
    domain_name = string
    region      = string
    weight      = number
  }))
}

# SSL Certificate for CloudFront (must be in us-east-1)
resource "aws_acm_certificate" "main" {
  domain_name       = "${var.project_name}.com"
  subject_alternative_names = [
    "*.${var.project_name}.com",
    "api.${var.project_name}.com",
    "ws.${var.project_name}.com"
  ]
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }

  tags = {
    Name = "${var.project_name}-cloudfront-cert"
    Environment = var.environment
  }
}

# WAF Web ACL for security
resource "aws_wafv2_web_acl" "main" {
  name  = "${var.project_name}-waf"
  scope = "CLOUDFRONT"

  default_action {
    allow {}
  }

  # Rate limiting rule
  rule {
    name     = "RateLimitRule"
    priority = 1

    action {
      block {}
    }

    statement {
      rate_based_statement {
        limit              = 2000
        aggregate_key_type = "IP"
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = true
      metric_name                = "RateLimitRule"
      sampled_requests_enabled   = true
    }
  }

  # Geographic blocking rule (if needed for compliance)
  rule {
    name     = "GeoBlockRule"
    priority = 2

    action {
      allow {}
    }

    statement {
      geo_match_statement {
        country_codes = ["US", "CA", "GB", "DE", "FR", "JP", "SG", "AU"]
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = true
      metric_name                = "GeoBlockRule"
      sampled_requests_enabled   = true
    }
  }

  # Common attack protection
  rule {
    name     = "CommonAttackProtection"
    priority = 3

    override_action {
      none {}
    }

    statement {
      managed_rule_group_statement {
        name        = "AWSManagedRulesCommonRuleSet"
        vendor_name = "AWS"
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = true
      metric_name                = "CommonAttackProtection"
      sampled_requests_enabled   = true
    }
  }

  # Known bad inputs protection
  rule {
    name     = "KnownBadInputsProtection"
    priority = 4

    override_action {
      none {}
    }

    statement {
      managed_rule_group_statement {
        name        = "AWSManagedRulesKnownBadInputsRuleSet"
        vendor_name = "AWS"
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = true
      metric_name                = "KnownBadInputsProtection"
      sampled_requests_enabled   = true
    }
  }

  tags = {
    Name = "${var.project_name}-waf"
    Environment = var.environment
  }
}

# CloudFront Origin Request Policy for API calls
resource "aws_cloudfront_origin_request_policy" "api_policy" {
  name = "${var.project_name}-api-policy"

  cookies_config {
    cookie_behavior = "none"
  }

  headers_config {
    header_behavior = "whitelist"
    headers {
      items = [
        "Authorization",
        "Content-Type",
        "Origin",
        "Referer",
        "User-Agent",
        "X-API-Key",
        "X-Forwarded-For"
      ]
    }
  }

  query_strings_config {
    query_string_behavior = "all"
  }
}

# CloudFront Cache Policy for static assets
resource "aws_cloudfront_cache_policy" "static_assets" {
  name = "${var.project_name}-static-assets"

  default_ttl = 86400
  max_ttl     = 31536000
  min_ttl     = 0

  parameters_in_cache_key_and_forwarded_to_origin {
    enable_accept_encoding_brotli = true
    enable_accept_encoding_gzip   = true

    cookies_config {
      cookie_behavior = "none"
    }

    headers_config {
      header_behavior = "none"
    }

    query_strings_config {
      query_string_behavior = "none"
    }
  }
}

# CloudFront Cache Policy for API responses
resource "aws_cloudfront_cache_policy" "api_cache" {
  name = "${var.project_name}-api-cache"

  default_ttl = 0
  max_ttl     = 300
  min_ttl     = 0

  parameters_in_cache_key_and_forwarded_to_origin {
    enable_accept_encoding_brotli = true
    enable_accept_encoding_gzip   = true

    cookies_config {
      cookie_behavior = "none"
    }

    headers_config {
      header_behavior = "whitelist"
      headers {
        items = ["Authorization", "X-API-Key"]
      }
    }

    query_strings_config {
      query_string_behavior = "all"
    }
  }
}

# Response Headers Policy for security
resource "aws_cloudfront_response_headers_policy" "security_headers" {
  name = "${var.project_name}-security-headers"

  security_headers_config {
    strict_transport_security {
      access_control_max_age_sec = 31536000
      include_subdomains         = true
      override                   = true
    }

    content_type_options {
      override = true
    }

    frame_options {
      frame_option = "DENY"
      override     = true
    }

    referrer_policy {
      referrer_policy = "strict-origin-when-cross-origin"
      override        = true
    }
  }

  custom_headers_config {
    items {
      header   = "X-API-Version"
      value    = "1.0"
      override = false
    }

    items {
      header   = "X-Trading-Platform"
      value    = "Bloomberg-Killer"
      override = false
    }
  }
}

# Main CloudFront Distribution
resource "aws_cloudfront_distribution" "main" {
  # Primary origin (US-East-1)
  origin {
    domain_name = var.regional_origins.primary.domain_name
    origin_id   = "primary-${var.regional_origins.primary.region}"

    custom_origin_config {
      http_port              = 80
      https_port             = 443
      origin_protocol_policy = "https-only"
      origin_ssl_protocols   = ["TLSv1.2"]
    }

    origin_shield {
      enabled              = true
      origin_shield_region = var.regional_origins.primary.region
    }
  }

  # Secondary origins for failover and geographic distribution
  dynamic "origin" {
    for_each = { for k, v in var.regional_origins : k => v if k != "primary" }
    
    content {
      domain_name = origin.value.domain_name
      origin_id   = "${origin.key}-${origin.value.region}"

      custom_origin_config {
        http_port              = 80
        https_port             = 443
        origin_protocol_policy = "https-only"
        origin_ssl_protocols   = ["TLSv1.2"]
      }

      origin_shield {
        enabled              = true
        origin_shield_region = origin.value.region
      }
    }
  }

  # Origin Groups for automatic failover
  origin_group {
    origin_id = "primary-group"

    failover_criteria {
      status_codes = [403, 404, 500, 502, 503, 504]
    }

    member {
      origin_id = "primary-${var.regional_origins.primary.region}"
    }

    member {
      origin_id = "secondary-${var.regional_origins.secondary.region}"
    }
  }

  enabled             = true
  is_ipv6_enabled     = true
  default_root_object = "index.html"
  web_acl_id          = aws_wafv2_web_acl.main.arn

  aliases = [
    "${var.project_name}.com",
    "api.${var.project_name}.com",
    "ws.${var.project_name}.com"
  ]

  # Default behavior for web app
  default_cache_behavior {
    allowed_methods            = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods             = ["GET", "HEAD", "OPTIONS"]
    target_origin_id           = "primary-group"
    compress                   = true
    viewer_protocol_policy     = "redirect-to-https"

    cache_policy_id            = aws_cloudfront_cache_policy.static_assets.id
    response_headers_policy_id = aws_cloudfront_response_headers_policy.security_headers.id

    # Lambda@Edge for intelligent routing
    lambda_function_association {
      event_type   = "origin-request"
      lambda_arn   = aws_lambda_function.edge_router.qualified_arn
      include_body = false
    }
  }

  # API behavior with minimal caching
  ordered_cache_behavior {
    path_pattern               = "/api/*"
    allowed_methods            = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods             = ["GET", "HEAD", "OPTIONS"]
    target_origin_id           = "primary-group"
    compress                   = true
    viewer_protocol_policy     = "https-only"

    cache_policy_id            = aws_cloudfront_cache_policy.api_cache.id
    origin_request_policy_id   = aws_cloudfront_origin_request_policy.api_policy.id
    response_headers_policy_id = aws_cloudfront_response_headers_policy.security_headers.id

    # Real-time API routing
    lambda_function_association {
      event_type   = "origin-request"
      lambda_arn   = aws_lambda_function.api_router.qualified_arn
      include_body = true
    }
  }

  # WebSocket behavior (no caching)
  ordered_cache_behavior {
    path_pattern               = "/ws/*"
    allowed_methods            = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods             = ["GET", "HEAD"]
    target_origin_id           = "primary-group"
    compress                   = false
    viewer_protocol_policy     = "https-only"

    # No caching for real-time data
    forwarded_values {
      query_string = true
      headers      = ["*"]
      cookies {
        forward = "all"
      }
    }

    min_ttl     = 0
    default_ttl = 0
    max_ttl     = 0

    # WebSocket routing
    lambda_function_association {
      event_type   = "origin-request"
      lambda_arn   = aws_lambda_function.websocket_router.qualified_arn
      include_body = false
    }
  }

  # Geographic restrictions (if needed)
  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  # SSL/TLS configuration
  viewer_certificate {
    acm_certificate_arn      = aws_acm_certificate.main.arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }

  # Price class for global performance
  price_class = "PriceClass_All"

  tags = {
    Name = "${var.project_name}-distribution"
    Environment = var.environment
  }
}

# Lambda@Edge function for intelligent routing
resource "aws_lambda_function" "edge_router" {
  provider         = aws.us-east-1  # Lambda@Edge must be in us-east-1
  filename         = "edge_router.zip"
  function_name    = "${var.project_name}-edge-router"
  role            = aws_iam_role.lambda_edge.arn
  handler         = "index.handler"
  source_code_hash = data.archive_file.edge_router.output_base64sha256
  runtime         = "nodejs18.x"
  timeout         = 5

  publish = true

  tags = {
    Name = "${var.project_name}-edge-router"
    Environment = var.environment
  }
}

# Lambda@Edge function for API routing
resource "aws_lambda_function" "api_router" {
  provider         = aws.us-east-1
  filename         = "api_router.zip"
  function_name    = "${var.project_name}-api-router"
  role            = aws_iam_role.lambda_edge.arn
  handler         = "index.handler"
  source_code_hash = data.archive_file.api_router.output_base64sha256
  runtime         = "nodejs18.x"
  timeout         = 5

  publish = true

  tags = {
    Name = "${var.project_name}-api-router"
    Environment = var.environment
  }
}

# Lambda@Edge function for WebSocket routing
resource "aws_lambda_function" "websocket_router" {
  provider         = aws.us-east-1
  filename         = "websocket_router.zip"
  function_name    = "${var.project_name}-websocket-router"
  role            = aws_iam_role.lambda_edge.arn
  handler         = "index.handler"
  source_code_hash = data.archive_file.websocket_router.output_base64sha256
  runtime         = "nodejs18.x"
  timeout         = 5

  publish = true

  tags = {
    Name = "${var.project_name}-websocket-router"
    Environment = var.environment
  }
}

# IAM role for Lambda@Edge
resource "aws_iam_role" "lambda_edge" {
  name = "${var.project_name}-lambda-edge-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = [
            "lambda.amazonaws.com",
            "edgelambda.amazonaws.com"
          ]
        }
      }
    ]
  })

  tags = {
    Name = "${var.project_name}-lambda-edge-role"
    Environment = var.environment
  }
}

# IAM policy for Lambda@Edge
resource "aws_iam_role_policy_attachment" "lambda_edge_policy" {
  role       = aws_iam_role.lambda_edge.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

# Archive files for Lambda functions (placeholder)
data "archive_file" "edge_router" {
  type        = "zip"
  output_path = "edge_router.zip"
  source {
    content = <<EOF
exports.handler = async (event) => {
    const request = event.Records[0].cf.request;
    
    // Intelligent routing based on user location and performance
    const countryCode = request.headers['cloudfront-viewer-country'][0].value;
    
    // Route to closest region for optimal performance
    if (countryCode === 'US' || countryCode === 'CA') {
        request.origin.custom.domainName = '${var.regional_origins.primary.domain_name}';
    } else if (['GB', 'DE', 'FR', 'IT', 'ES'].includes(countryCode)) {
        request.origin.custom.domainName = '${var.regional_origins.europe.domain_name}';
    } else if (['JP', 'SG', 'AU', 'KR'].includes(countryCode)) {
        request.origin.custom.domainName = '${var.regional_origins.asia.domain_name}';
    }
    
    return request;
};
EOF
    filename = "index.js"
  }
}

data "archive_file" "api_router" {
  type        = "zip"
  output_path = "api_router.zip"
  source {
    content = <<EOF
exports.handler = async (event) => {
    const request = event.Records[0].cf.request;
    
    // Add performance headers for API monitoring
    request.headers['x-edge-location'] = [{ key: 'X-Edge-Location', value: 'GLOBAL' }];
    request.headers['x-request-id'] = [{ key: 'X-Request-ID', value: Math.random().toString(36) }];
    
    return request;
};
EOF
    filename = "index.js"
  }
}

data "archive_file" "websocket_router" {
  type        = "zip"
  output_path = "websocket_router.zip"
  source {
    content = <<EOF
exports.handler = async (event) => {
    const request = event.Records[0].cf.request;
    
    // Route WebSocket connections to primary region for consistency
    request.origin.custom.domainName = '${var.regional_origins.primary.domain_name}';
    
    return request;
};
EOF
    filename = "index.js"
  }
}

# Outputs
output "distribution_id" {
  description = "CloudFront distribution ID"
  value       = aws_cloudfront_distribution.main.id
}

output "distribution_domain_name" {
  description = "CloudFront distribution domain name"
  value       = aws_cloudfront_distribution.main.domain_name
}

output "distribution_arn" {
  description = "CloudFront distribution ARN"
  value       = aws_cloudfront_distribution.main.arn
}

output "waf_web_acl_arn" {
  description = "WAF Web ACL ARN"
  value       = aws_wafv2_web_acl.main.arn
}