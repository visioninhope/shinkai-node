#!/bin/bash

export NODE_IP="0.0.0.0"
export NODE_PORT="9552"
export NODE_API_IP="0.0.0.0"
export NODE_API_PORT="9550"
export IDENTITY_SECRET_KEY="df3f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81119"
export ENCRYPTION_SECRET_KEY="d83f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81159"
export PING_INTERVAL_SECS="0"
export GLOBAL_IDENTITY_NAME="@@localhost.shinkai"
export RUST_LOG=debug,error,info
export STARTING_NUM_QR_PROFILES="1"
export STARTING_NUM_QR_DEVICES="1"
export FIRST_DEVICE_NEEDS_REGISTRATION_CODE="false"
export LOG_SIMPLE="true"
export NO_SECRET_FILE="true"
export EMBEDDINGS_SERVER_URL="https://internal.shinkai.com/x-embed-api/"
export UNSTRUCTURED_SERVER_URL="https://internal.shinkai.com/x-unstructured-api/"

export INITIAL_AGENT_NAMES="premium_gpt,standard_mixtral,gpt_vision"
export INITIAL_AGENT_URLS="https://backend-node-b1.shinkai.com,https://backend-node-b1.shinkai.com,https://backend-node-b1.shinkai.com"
export INITIAL_AGENT_MODELS="shinkai-backend:PREMIUM_TEXT_INFERENCE,shinkai-backend:STANDARD_TEXT_INFERENCE,shinkai-backend:PREMIUM_VISION_INFERENCE"

# Add these lines to enable all log options
export LOG_ALL=1

cargo run
