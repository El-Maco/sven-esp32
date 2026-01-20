#!/bin/bash

sudo modprobe cdc_acm
sudo chown $USER:$USER /dev/ttyACM0
