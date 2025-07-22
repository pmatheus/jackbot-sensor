# Bloomberg Killer - Production Infrastructure
# Multi-region AWS deployment with 99.99% uptime capability

terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
  backend "s3" {
    bucket = "jackbot-terraform-state"
    key    = "production/terraform.tfstate"
    region = "us-east-1"
    dynamodb_table = "jackbot-terraform-locks"
    encrypt = true
  }
}

# Configure AWS Providers for Multi-Region Deployment
provider "aws" {
  alias  = "primary"
  region = "us-east-1"
  
  default_tags {
    tags = {
      Project     = "jackbot-bloomberg-killer"
      Environment = "production"
      ManagedBy   = "terraform"
      CostCenter  = "trading-platform"
    }
  }
}

provider "aws" {
  alias  = "secondary"
  region = "us-west-2"
  
  default_tags {
    tags = {
      Project     = "jackbot-bloomberg-killer"
      Environment = "production"
      ManagedBy   = "terraform"
      CostCenter  = "trading-platform"
    }
  }
}

provider "aws" {
  alias  = "europe"
  region = "eu-west-1"
  
  default_tags {
    tags = {
      Project     = "jackbot-bloomberg-killer"
      Environment = "production"
      ManagedBy   = "terraform"
      CostCenter  = "trading-platform"
    }
  }
}

provider "aws" {
  alias  = "asia"
  region = "ap-southeast-1"
  
  default_tags {
    tags = {
      Project     = "jackbot-bloomberg-killer"
      Environment = "production"
      ManagedBy   = "terraform"
      CostCenter  = "trading-platform"
    }
  }
}

# Local variables for configuration
locals {
  regions = {
    primary   = "us-east-1"
    secondary = "us-west-2"
    europe    = "eu-west-1"
    asia      = "ap-southeast-1"
  }
  
  # Performance targets for financial trading
  performance_targets = {
    api_latency_p95 = "50ms"
    api_latency_p99 = "100ms"
    websocket_latency = "10ms"
    uptime_sla = "99.99%"
  }
  
  # Environment configuration
  environment = "production"
  project_name = "jackbot-bloomberg-killer"
}

# Primary Region (US-East-1) - Trading Hub
module "primary_region" {
  source = "./modules/region"
  providers = {
    aws = aws.primary
  }
  
  region = local.regions.primary
  environment = local.environment
  project_name = local.project_name
  is_primary = true
  
  # Enhanced capacity for primary trading region
  database_config = {
    instance_class = "db.r6g.2xlarge"
    allocated_storage = 1000
    max_allocated_storage = 10000
    multi_az = true
    backup_retention_period = 35
    backup_window = "03:00-04:00"
    maintenance_window = "sun:04:00-sun:05:00"
  }
  
  cache_config = {
    node_type = "cache.r6g.2xlarge"
    num_cache_nodes = 3
    parameter_group_name = "default.redis7"
  }
  
  ecs_config = {
    cpu = 4096
    memory = 8192
    desired_count = 6
    max_capacity = 20
    min_capacity = 3
  }
}

# Secondary Region (US-West-2) - Disaster Recovery
module "secondary_region" {
  source = "./modules/region"
  providers = {
    aws = aws.secondary
  }
  
  region = local.regions.secondary
  environment = local.environment
  project_name = local.project_name
  is_primary = false
  
  database_config = {
    instance_class = "db.r6g.xlarge"
    allocated_storage = 500
    max_allocated_storage = 5000
    multi_az = true
    backup_retention_period = 35
    backup_window = "06:00-07:00"
    maintenance_window = "sun:07:00-sun:08:00"
  }
  
  cache_config = {
    node_type = "cache.r6g.xlarge"
    num_cache_nodes = 2
    parameter_group_name = "default.redis7"
  }
  
  ecs_config = {
    cpu = 2048
    memory = 4096
    desired_count = 3
    max_capacity = 10
    min_capacity = 2
  }
}

# Europe Region (EU-West-1) - European Markets
module "europe_region" {
  source = "./modules/region"
  providers = {
    aws = aws.europe
  }
  
  region = local.regions.europe
  environment = local.environment
  project_name = local.project_name
  is_primary = false
  
  database_config = {
    instance_class = "db.r6g.xlarge"
    allocated_storage = 500
    max_allocated_storage = 5000
    multi_az = true
    backup_retention_period = 35
    backup_window = "02:00-03:00"
    maintenance_window = "sun:03:00-sun:04:00"
  }
  
  cache_config = {
    node_type = "cache.r6g.xlarge"
    num_cache_nodes = 2
    parameter_group_name = "default.redis7"
  }
  
  ecs_config = {
    cpu = 2048
    memory = 4096
    desired_count = 3
    max_capacity = 10
    min_capacity = 2
  }
}

# Asia Region (AP-Southeast-1) - Asian Markets
module "asia_region" {
  source = "./modules/region"
  providers = {
    aws = aws.asia
  }
  
  region = local.regions.asia
  environment = local.environment
  project_name = local.project_name
  is_primary = false
  
  database_config = {
    instance_class = "db.r6g.xlarge"
    allocated_storage = 500
    max_allocated_storage = 5000
    multi_az = true
    backup_retention_period = 35
    backup_window = "20:00-21:00"
    maintenance_window = "sun:21:00-sun:22:00"
  }
  
  cache_config = {
    node_type = "cache.r6g.xlarge"
    num_cache_nodes = 2
    parameter_group_name = "default.redis7"
  }
  
  ecs_config = {
    cpu = 2048
    memory = 4096
    desired_count = 3
    max_capacity = 10
    min_capacity = 2
  }
}

# Global CloudFront Distribution
module "global_cdn" {
  source = "./modules/cloudfront"
  providers = {
    aws = aws.primary
  }
  
  project_name = local.project_name
  environment = local.environment
  
  # Origin configuration for all regions
  regional_origins = {
    primary = {
      domain_name = module.primary_region.alb_dns_name
      region = local.regions.primary
      weight = 100
    }
    secondary = {
      domain_name = module.secondary_region.alb_dns_name
      region = local.regions.secondary
      weight = 0
    }
    europe = {
      domain_name = module.europe_region.alb_dns_name
      region = local.regions.europe
      weight = 50
    }
    asia = {
      domain_name = module.asia_region.alb_dns_name
      region = local.regions.asia
      weight = 50
    }
  }
}

# Route53 Health Checks and DNS Failover
module "dns_failover" {
  source = "./modules/route53"
  providers = {
    aws = aws.primary
  }
  
  project_name = local.project_name
  environment = local.environment
  
  regional_endpoints = {
    primary = {
      endpoint = module.primary_region.alb_dns_name
      region = local.regions.primary
      failover_type = "PRIMARY"
    }
    secondary = {
      endpoint = module.secondary_region.alb_dns_name
      region = local.regions.secondary
      failover_type = "SECONDARY"
    }
    europe = {
      endpoint = module.europe_region.alb_dns_name
      region = local.regions.europe
      failover_type = "PRIMARY"
    }
    asia = {
      endpoint = module.asia_region.alb_dns_name
      region = local.regions.asia
      failover_type = "PRIMARY"
    }
  }
}

# Cross-region replication for RDS
resource "aws_db_instance" "cross_region_read_replica" {
  count = 3
  
  identifier = "jackbot-read-replica-${count.index + 1}"
  replicate_source_db = module.primary_region.rds_identifier
  
  instance_class = "db.r6g.xlarge"
  publicly_accessible = false
  auto_minor_version_upgrade = true
  
  # Distribute replicas across regions
  availability_zone = count.index == 0 ? "us-west-2a" : count.index == 1 ? "eu-west-1a" : "ap-southeast-1a"
  
  tags = {
    Name = "jackbot-read-replica-${count.index + 1}"
    Environment = local.environment
    Purpose = "read-scaling"
  }
}

# Outputs for other modules and monitoring
output "regional_endpoints" {
  description = "Regional API endpoints for the Bloomberg killer platform"
  value = {
    primary = module.primary_region.api_endpoint
    secondary = module.secondary_region.api_endpoint
    europe = module.europe_region.api_endpoint
    asia = module.asia_region.api_endpoint
  }
}

output "database_endpoints" {
  description = "Database connection endpoints"
  value = {
    primary_writer = module.primary_region.rds_endpoint
    primary_reader = module.primary_region.rds_reader_endpoint
    read_replicas = aws_db_instance.cross_region_read_replica[*].endpoint
  }
  sensitive = true
}

output "cloudfront_distribution" {
  description = "Global CDN endpoint"
  value = module.global_cdn.distribution_domain_name
}

output "monitoring_endpoints" {
  description = "Monitoring and alerting endpoints"
  value = {
    primary_region_dashboard = module.primary_region.monitoring_dashboard_url
    global_performance_dashboard = "https://cloudwatch.amazonaws.com/dashboard/${local.project_name}-global"
  }
}