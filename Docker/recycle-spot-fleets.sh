#!/bin/bash

set -euo pipefail

AWS_REGION=${AWS_REGION:-"eu-west-1"}
COORDINATOR_STACK=${COORDINATOR_STACK:-"DistributedColonyCoordinator"}
BACKEND_STACK=${BACKEND_STACK:-"DistributedColonyBackend"}
COORDINATOR_TARGET=${COORDINATOR_TARGET:-1}
BACKEND_TARGET=${BACKEND_TARGET:-2}

INFO() { echo "[INFO] $*"; }
WARN() { echo "[WARN] $*"; }
ERROR() { echo "[ERROR] $*" >&2; }

get_fleet_id() {
  local stack=$1
  local output_key=$2
  aws cloudformation describe-stacks \
    --stack-name "$stack" \
    --region "$AWS_REGION" \
    --query "Stacks[0].Outputs[?OutputKey==\`$output_key\`].OutputValue" \
    --output text
}

scale_fleet() {
  local fleet_id=$1
  local target=$2
  aws ec2 modify-spot-fleet-request \
    --spot-fleet-request-id "$fleet_id" \
    --target-capacity "$target" \
    --region "$AWS_REGION"
}

INFO "Retrieving coordinator Spot Fleet ID..."
COORD_FLEET_ID=$(get_fleet_id "$COORDINATOR_STACK" "CoordinatorSpotFleetId")
if [ -z "$COORD_FLEET_ID" ] || [ "$COORD_FLEET_ID" = "None" ]; then
  WARN "Coordinator stack '$COORDINATOR_STACK' not found or missing CoordinatorSpotFleetId output. Skipping coordinator recycle."
  COORD_FLEET_ID=""
else
  INFO "Coordinator fleet: $COORD_FLEET_ID"
fi

if [ -n "$BACKEND_STACK" ]; then
  INFO "Retrieving backend Spot Fleet ID..."
  BACKEND_FLEET_ID=$(get_fleet_id "$BACKEND_STACK" "SpotFleetId")
  if [ -z "$BACKEND_FLEET_ID" ] || [ "$BACKEND_FLEET_ID" = "None" ]; then
    WARN "Backend stack '$BACKEND_STACK' not found or missing SpotFleetId output. Skipping backend recycle."
    BACKEND_FLEET_ID=""
  else
    INFO "Backend fleet: $BACKEND_FLEET_ID"
  fi
else
  BACKEND_FLEET_ID=""
fi

if [ -z "$COORD_FLEET_ID" ] && [ -z "$BACKEND_FLEET_ID" ]; then
  ERROR "No spot fleets found. Nothing to recycle."
  exit 1
fi

if [ -n "$COORD_FLEET_ID" ]; then
  INFO "Scaling coordinator fleet to 0 (terminating instances)..."
  scale_fleet "$COORD_FLEET_ID" 0
fi

if [ -n "$BACKEND_FLEET_ID" ]; then
  INFO "Scaling backend fleet to 0 (terminating instances)..."
  scale_fleet "$BACKEND_FLEET_ID" 0
fi

INFO "Waiting 60 seconds for instances to terminate..."
sleep 60

if [ -n "$COORD_FLEET_ID" ]; then
  INFO "Restoring coordinator fleet to target $COORDINATOR_TARGET..."
  scale_fleet "$COORD_FLEET_ID" "$COORDINATOR_TARGET"
fi

if [ -n "$BACKEND_FLEET_ID" ]; then
  INFO "Restoring backend fleet to target $BACKEND_TARGET..."
  scale_fleet "$BACKEND_FLEET_ID" "$BACKEND_TARGET"
fi

INFO "Recycle request submitted. Instances will relaunch with updated configuration."

