#!/bin/bash
set -euo pipefail

# Hardcoded configuration
STACK_NAME="DistributedColonyStack"                          # CloudFormation stack name
KEY_NAME="distributed-colony-key"                         # EC2 key pair name (must exist in the region)
KEY_PATH="distributed-colony-key.pem"        # Path to your .pem file
REGION="eu-west-1"                                       # AWS region

echo "[INFO] Using region: ${REGION}"
echo "[INFO] Using key: ${KEY_NAME} at ${KEY_PATH}"
echo "[INFO] Looking up BackendLaunchTemplate in stack: ${STACK_NAME}"

# Expand tilde in KEY_PATH if present
EXPANDED_KEY_PATH="${KEY_PATH/#~/$HOME}"

if [[ ! -f "${EXPANDED_KEY_PATH}" ]]; then
  echo "[ERROR] SSH key file not found: ${EXPANDED_KEY_PATH}"
  exit 1
fi

LT_ID=$(aws cloudformation describe-stack-resources \
  --stack-name "${STACK_NAME}" \
  --logical-resource-id BackendLaunchTemplate \
  --query 'StackResources[0].PhysicalResourceId' \
  --output text \
  --region "${REGION}")

if [[ -z "${LT_ID}" || "${LT_ID}" == "None" ]]; then
  echo "[ERROR] Could not find BackendLaunchTemplate in stack ${STACK_NAME}."
  exit 1
fi

echo "[INFO] Launch Template ID: ${LT_ID}"

INSTANCE_ID=$(aws ec2 describe-instances \
  --filters Name=instance-state-name,Values=running Name=launch-template.launch-template-id,Values="${LT_ID}" \
  --query 'Reservations[].Instances[].InstanceId' \
  --output text \
  --region "${REGION}" | awk '{print $1}')

if [[ -z "${INSTANCE_ID}" || "${INSTANCE_ID}" == "None" ]]; then
  echo "[ERROR] No running instances found for launch template ${LT_ID}."
  exit 1
fi

echo "[INFO] Using Instance ID: ${INSTANCE_ID}"

PUBLIC_IP=$(aws ec2 describe-instances \
  --instance-ids "${INSTANCE_ID}" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' \
  --output text \
  --region "${REGION}")

if [[ -z "${PUBLIC_IP}" || "${PUBLIC_IP}" == "None" ]]; then
  echo "[ERROR] Instance ${INSTANCE_ID} does not have a public IP."
  exit 1
fi

echo "[INFO] Public IP: ${PUBLIC_IP}"
echo "[INFO] Connecting via SSH (user: ec2-user)"

ssh -o StrictHostKeyChecking=no -i "${EXPANDED_KEY_PATH}" ec2-user@"${PUBLIC_IP}"


