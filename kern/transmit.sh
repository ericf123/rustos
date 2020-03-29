echo /dev/ttyUSB$1
ttywrite -i ./build/kernel.bin /dev/ttyUSB$1
