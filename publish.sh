#!/bin/bash
set -e
cd "$(dirname "$0")"
vsce publish patch

