struct Frame { vp:mat4x4<f32>, inverse:mat4x4<f32>, eye_time:vec4<f32>, start_charge:vec4<f32>, end_hit:vec4<f32>, options:vec4<f32> }
struct Particle { position_size:vec4<f32>, color:vec4<f32>, rotation:vec4<f32> }
@group(0) @binding(0) var<uniform> f:Frame;
@group(0) @binding(1) var<storage,read> particles:array<Particle>;
@group(0) @binding(2) var scene_depth:texture_depth_2d;
struct Full { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn fullscreen(@builtin(vertex_index) i:u32)->Full {
    let xy=vec2(f32((i<<1u)&2u),f32(i&2u));var o:Full;o.position=vec4(xy*2.0-1.0,0.0,1.0);o.uv=vec2(xy.x,1.0-xy.y);return o;
}
fn world(ndc:vec2<f32>,depth:f32)->vec3<f32> { let p=f.inverse*vec4(ndc,depth,1.0);return p.xyz/p.w; }
@fragment fn lance(v:Full)->@location(0) vec4<f32> {
    let eye=f.eye_time.xyz;let t=f.eye_time.w;let release=t-f.start_charge.w;
    let start=f.start_charge.xyz;let end=f.end_hit.xyz;let axis=normalize(end-start);
    let ndc=vec2(v.uv.x*2.0-1.0,1.0-v.uv.y*2.0);
    let ray=normalize(world(ndc,0.001)-eye);
    let depth=textureLoad(scene_depth,vec2<i32>(v.position.xy),0);
    let limit=select(10000.0,length(world(ndc,max(depth,0.000001))-eye),depth>0.0);
    var glow=vec3(0.0);let progress=clamp(t/f.start_charge.w,0.0,1.0);
    // A rotating multi-ring sigil perpendicular to the authoritative beam.
    let denom=dot(ray,axis);let plane=dot(start-eye,axis)/select(0.00001,denom,abs(denom)>0.00001);
    if plane>0.0&&plane<limit+0.05 {
        let p=eye+ray*plane-start;let radius=length(p);
        let radius_scale=1.0+progress*1.3;
        let rings=exp(-pow((radius-radius_scale)*55.0,2.0))+0.55*exp(-pow((radius-radius_scale*0.79)*65.0,2.0));
        let right=normalize(cross(axis,select(vec3(0.0,1.0,0.0),vec3(1.0,0.0,0.0),abs(axis.y)>0.95)));
        let angle=atan2(dot(p,cross(right,axis)),dot(p,right))+t*0.3;
        let marks=pow(max(0.0,cos(angle*18.0)),32.0)*exp(-pow((radius-radius_scale*0.9)*20.0,2.0));
        let fade=select(progress,exp(-max(0.0,release)*4.0),release>=0.0);
        glow+=(rings+marks)*vec3(3.0,1.0,0.16)*fade;
    }
    if release>=0.0 {
        // Closest points of the view ray and finite beam: a subpixel core is
        // stable without a costly full-screen volume march.
        let w=eye-start;let b=dot(ray,axis);let d=dot(ray,w);let e=dot(axis,w);
        var along=clamp((e-b*d)/max(0.0001,1.0-b*b),0.0,length(end-start));
        let ray_t=dot(start+axis*along-eye,ray);
        let distance=length(eye+ray*ray_t-start-axis*along);
        let fade=exp(-release*4.0);
        if ray_t>0.0&&ray_t<limit+0.05 {
            let turbulent=1.0+0.22*sin(along*12.0-t*40.0);
            glow+=(exp(-distance*distance*180.0)*vec3(8.0,5.6,2.6)+exp(-distance*distance*5.0*turbulent)*vec3(2.2,0.45,0.025))*fade;
        }
        if f.end_hit.w>0.5 {
            let center=end;let hit_t=dot(center-eye,ray);let dist=length(eye+ray*hit_t-center);
            if hit_t>0.0&&hit_t<limit+3.0 {
                glow+=exp(-dist*dist/max(0.02,release*3.0))*vec3(8.0,3.0,0.4)*exp(-release*4.0);
                let shell=release*14.0;
                glow+=exp(-pow((dist-shell)*7.0,2.0))*vec3(1.8,0.6,0.09)*exp(-release*2.2);
            }
            let floor_t=(center.y-0.8-eye.y)/ray.y;
            if floor_t>0.0&&floor_t<limit+1.0 {
                let p=eye+ray*floor_t;let r=length(p.xz-center.xz);
                glow+=exp(-pow((r-release*12.0)*7.0,2.0))*vec3(5.0,1.1,0.06)*exp(-release*1.8);
            }
        }
    }
    return vec4(glow*mix(0.16,1.0,f.options.y),0.0);
}
struct Sprite { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) color:vec4<f32> }
@vertex fn sprite_vertex(@builtin(vertex_index) vertex:u32,@builtin(instance_index) instance:u32)->Sprite {
    let corners=array<vec2<f32>,6>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),vec2(-1.0,1.0),vec2(1.0,-1.0),vec2(1.0,1.0));
    let p=particles[instance];let uv=corners[vertex];let angle=p.rotation.x;
    let rotated=mat2x2(cos(angle),sin(angle),-sin(angle),cos(angle))*uv;
    var clip=f.vp*vec4(p.position_size.xyz,1.0);
    // Bound projected area to 10 pixels in radius to contain overdraw at the camera.
    let radius=min(p.position_size.w*2.0,max(0.0,clip.w)*20.0/f.options.w);
    clip.x+=rotated.x*radius;clip.y+=rotated.y*radius;
    var o:Sprite;o.position=clip;o.uv=uv;o.color=p.color*mix(0.16,1.0,f.options.y);return o;
}
@fragment fn sprite_fragment(v:Sprite)->@location(0) vec4<f32> {
    let r=dot(v.uv,v.uv);if r>1.0 { discard; }
    let a=exp(-r*5.0)*pow(1.0-r,2.0);return vec4(v.color.rgb*a,0.0);
}
