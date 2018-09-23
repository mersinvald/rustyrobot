#!/bin/bash

ZOOKEEPER="localhost:2181"

function create_topic() {
        echo "Creating topic $1 with $2 partitions of replication factor 1"
	kafka-topics.sh --create \
	    --zookeeper $ZOOKEEPER \
	    --topic $1 \
	    --partitions $2 \
	    --replication-factor 1
}

# Github microservice input & output
create_topic "github.request" 1
create_topic "github.event" 1


