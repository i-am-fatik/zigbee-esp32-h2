import {identify, onOff} from 'zigbee-herdsman-converters/lib/modernExtend';

export default {
    zigbeeModel: ['H2.NoStd.Light'],
    model: 'H2.NoStd.Light',
    vendor: 'esp-rs',
    description: 'ESP32-H2 no_std Rust Zigbee light',
    extend: [identify(), onOff({powerOnBehavior: false})],
};
