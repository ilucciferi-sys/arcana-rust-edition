struct Frame { vp:mat4x4<f32>, inverse:mat4x4<f32>, eye_time:vec4<f32>, start_charge:vec4<f32>, end_hit:vec4<f32>, options:vec4<f32> }
@group(0) @binding(0) var emission:texture_2d<f32>;
@group(0) @binding(1) var linear:sampler;
@group(0) @binding(2) var<uniform> f:Frame;
struct Full { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn fullscreen(@builtin(vertex_index) i:u32)->Full {
    let xy=vec2(f32((i<<1u)&2u),f32(i&2u));var o:Full;o.position=vec4(xy*2.0-1.0,0.0,1.0);o.uv=vec2(xy.x,1.0-xy.y);return o;
}
@fragment fn composite(v:Full)->@location(0) vec4<f32> {
    var color=textureSample(emission,linear,v.uv).rgb;
    let pixel=1.0/vec2<f32>(textureDimensions(emission));
    if f.options.z>0.5 {
        for(var i=0;i<12;i++) {
            let angle=f32(i)*2.399963;let offset=vec2(cos(angle),sin(angle))*(3.0+f32(i)*2.5)*pixel;
            let sample=textureSampleLevel(emission,linear,v.uv+offset,0.0).rgb;
            color+=max(vec3(0.0),sample-vec3(0.7))*0.09;
        }
    }
    // Emission remains HDR through bloom, then maps to the host's world format.
    return vec4(vec3(1.0)-exp(-color*0.65),0.0);
}
