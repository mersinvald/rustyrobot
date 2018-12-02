#!/bin/bash

KAFKA_TOPICS_CMD=""
echo $OSTYPE

if [[ "$OSTYPE" == "linux-gnu" ]]; then
        KAFKA_TOPICS_CMD="kafka-topics.sh"
elif [[ "$OSTYPE" == "darwin"* ]]; then
        KAFKA_TOPICS_CMD="kafka-topics"
fi

ZOOKEEPER="localhost:2181"

function create_topic() {
    echo "Creating topic $1 with $2 partitions of replication factor 1"
	$KAFKA_TOPICS_CMD --create \
	    --zookeeper $ZOOKEEPER \
	    --topic $1 \
	    --partitions $2 \
	    --replication-factor 1
}

function enable_compaction() {
   echo "Enabling log compaction on topic $1"
   $KAFKA_TOPICS_CMD \
        --zookeeper $ZOOKEEPER \
        --alter --topic $1 \
        --config cleanup.policy=compact
}

# Testing topics
create_topic "rustyrobot.test.state.save_and_restore" 1
create_topic "rustyrobot.test.handler.in" 1
create_topic "rustyrobot.test.handler.out" 1
enable_compaction "rustyrobot.test.state.save_and_restore"

# Common event bus
create_topic "rustyrobot.github.event" 1

# Github microservice input & output
create_topic "rustyrobot.github.request" 1
create_topic "rustyrobot.github.state" 1
enable_compaction "rustyrobot.github.state"

# Fetcher topics
create_topic "rustyrobot.fetcher.state" 1
enable_compaction "rustyrobot.fetcher.state"


