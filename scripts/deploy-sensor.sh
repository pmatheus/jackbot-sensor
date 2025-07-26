#!/bin/bash

# Deploy Jackbot Sensor
# This script builds and deploys the sensor Docker image

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
AWS_REGION=${AWS_REGION:-us-east-1}
ECR_REGISTRY=${ECR_REGISTRY:-""}
IMAGE_NAME="jackbot-sensor"
PLATFORMS="linux/amd64,linux/arm64"

# Parse arguments
ENVIRONMENT="dev"
PUSH_TO_ECR=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --env)
            ENVIRONMENT="$2"
            shift 2
            ;;
        --push)
            PUSH_TO_ECR=true
            shift
            ;;
        --registry)
            ECR_REGISTRY="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}Building Jackbot Sensor for ${ENVIRONMENT} environment${NC}"

# Build multi-platform Docker image
echo -e "${YELLOW}Building Docker image for platforms: ${PLATFORMS}${NC}"

if [ "$PUSH_TO_ECR" = true ]; then
    if [ -z "$ECR_REGISTRY" ]; then
        echo -e "${RED}ECR registry not specified. Use --registry option.${NC}"
        exit 1
    fi

    # Login to ECR
    echo -e "${YELLOW}Logging in to ECR...${NC}"
    aws ecr get-login-password --region $AWS_REGION | docker login --username AWS --password-stdin $ECR_REGISTRY

    # Build and push
    docker buildx build \
        --platform $PLATFORMS \
        --tag $ECR_REGISTRY/$IMAGE_NAME:latest \
        --tag $ECR_REGISTRY/$IMAGE_NAME:$(git rev-parse --short HEAD) \
        --push \
        .

    echo -e "${GREEN}Image pushed to ECR successfully${NC}"
else
    # Build locally
    docker buildx build \
        --platform linux/amd64 \
        --tag $IMAGE_NAME:latest \
        --load \
        .

    echo -e "${GREEN}Image built locally${NC}"
fi

# Deploy sensor management Lambda
if [ "$ENVIRONMENT" != "local" ]; then
    echo -e "${YELLOW}Deploying sensor management Lambda...${NC}"
    
    cd ../jackbot-backend/lambdas/sensor-management
    
    # Build Lambda
    cargo lambda build --release --arm64
    
    # Package Lambda
    cp target/lambda/sensor-management/bootstrap ./
    zip -j sensor-management.zip bootstrap
    rm bootstrap
    
    # Deploy or update Lambda
    FUNCTION_NAME="jackbot-sensor-management-${ENVIRONMENT}"
    
    if aws lambda get-function --function-name $FUNCTION_NAME --region $AWS_REGION 2>/dev/null; then
        echo -e "${YELLOW}Updating existing Lambda function...${NC}"
        aws lambda update-function-code \
            --function-name $FUNCTION_NAME \
            --zip-file fileb://sensor-management.zip \
            --region $AWS_REGION
    else
        echo -e "${YELLOW}Creating new Lambda function...${NC}"
        aws lambda create-function \
            --function-name $FUNCTION_NAME \
            --runtime provided.al2 \
            --role arn:aws:iam::$(aws sts get-caller-identity --query Account --output text):role/JackbotLambdaRole \
            --handler bootstrap \
            --zip-file fileb://sensor-management.zip \
            --timeout 60 \
            --memory-size 256 \
            --architectures arm64 \
            --environment Variables="{
                ECS_CLUSTER_NAME=jackbot-sensors-${ENVIRONMENT},
                TASK_DEFINITION=jackbot-sensor:latest,
                SUBNET_IDS=${SUBNET_IDS},
                SECURITY_GROUP_ID=${SECURITY_GROUP_ID},
                KAFKA_BROKERS=${KAFKA_BROKERS},
                REDIS_URL=${REDIS_URL}
            }" \
            --region $AWS_REGION
    fi
    
    # Clean up
    rm sensor-management.zip
    
    echo -e "${GREEN}Lambda deployed successfully${NC}"
fi

echo -e "${GREEN}Deployment complete!${NC}"

# Show next steps
echo -e "\n${YELLOW}Next steps:${NC}"
echo "1. Test locally: docker-compose -f docker-compose.sensor.yml up"
echo "2. Deploy sensor: aws lambda invoke --function-name $FUNCTION_NAME --payload '{\"action\":\"deploy\",\"exchange\":\"binance\",\"symbols\":[\"BTC/USDT\"]}' response.json"
echo "3. Check status: aws lambda invoke --function-name $FUNCTION_NAME --payload '{\"action\":\"status\"}' response.json"
echo "4. Monitor health: docker-compose -f docker-compose.sensor.yml logs health-monitor"