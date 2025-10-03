#!/bin/bash

# Script to create EC2 key pair for DistributedColony
# This will create a key pair in AWS and save the private key locally

set -e

KEY_NAME="distributed-colony-key"
PRIVATE_KEY_FILE="distributed-colony-key.pem"

echo "Creating EC2 key pair: $KEY_NAME"

# Create the key pair in AWS
aws ec2 create-key-pair \
  --key-name "$KEY_NAME" \
  --query 'KeyMaterial' \
  --output text > "$PRIVATE_KEY_FILE"

# Set proper permissions for the private key
chmod 400 "$PRIVATE_KEY_FILE"

echo "✅ Key pair created successfully!"
echo "📁 Private key saved to: $PRIVATE_KEY_FILE"
echo "🔑 Key name in AWS: $KEY_NAME"
echo ""
echo "To connect to your instance later, use:"
echo "ssh -i $PRIVATE_KEY_FILE ec2-user@<instance-ip>"
echo ""
echo "⚠️  IMPORTANT: Keep your private key secure and never commit it to git!"
