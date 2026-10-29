struct Frame { vp:mat4x4<f32>, inverse:mat4x4<f32>, eye_time:vec4<f32>, start_charge:vec4<f32>, end_hit:vec4<f32>, options:vec4<f32> }
struct Particle { position_size:vec4<f32>, color:vec4<f32>, rotation:vec4<f32> }
@group(0) @binding(0) var<uniform> f:Frame;
@group(0) @binding(1) var<storage,read_write> particles:array<Particle>;
fn random(i:u32)->f32 { var x=i*747796405u+2891336453u; x=((x>>((x>>28u)+4u))^x)*277803737u; return f32((x>>22u)^x)/4294967295.0; }
@compute @workgroup_size(128)
fn simulate(@builtin(global_invocation_id) id:vec3<u32>) {
    let i=id.x; if i>=u32(f.options.x) { return; }
    let a=random(i*7u+1u); let b=random(i*7u+2u); let c=random(i*7u+3u);
    let t=f.eye_time.w; let release=t-f.start_charge.w;
    let direction=normalize(f.end_hit.xyz-f.start_charge.xyz);
    let up=select(vec3(0.0,1.0,0.0),vec3(1.0,0.0,0.0),abs(direction.y)>0.95);
    let right=normalize(cross(direction,up));let vertical=cross(right,direction);
    let angle=a*6.2831853+t*(1.0+b*2.0);
    var position:vec3<f32>;var size=0.025+c*0.045; var alpha=0.0;
    if release<0.0 {
        let progress=clamp(t/f.start_charge.w,0.0,1.0);
        let travel=fract(b+t*(0.15+c*0.2));
        let r=(1.0-travel)*(2.0+progress*2.0);
        position=f.start_charge.xyz+(right*cos(angle)+vertical*sin(angle))*r-direction*(1.0-travel)*2.0;
        alpha=progress*(1.0-travel)*0.6;
    } else {
        let life=0.6+c*2.4;let age=max(0.0,release-b*0.25);
        let z=a*2.0-1.0;let h=sqrt(max(0.0,1.0-z*z));
        let radial=vec3(h*cos(b*6.2831853),z,h*sin(b*6.2831853));
        let velocity=radial*(2.0+c*10.0)+vec3(0.0,2.0,0.0);
        let anchor=select(mix(f.start_charge.xyz,f.end_hit.xyz,a),f.end_hit.xyz,i%3u!=0u&&f.end_hit.w>0.5);
        position=anchor+velocity*age+vec3(0.0,-2.0*age*age,0.0);
        alpha=pow(max(0.0,1.0-age/life),2.0)*0.8;
        size*=1.0+age*0.5;
    }
    // Keep brightness and covered area stable as quality increases. High/Ultra
    // add finer detail instead of a brighter opaque curtain over the target.
    let density=4096.0/f.options.x;
    alpha*=density*select(0.3,0.12,release<0.0);
    size*=sqrt(density)*0.8;
    particles[i].position_size=vec4(position,size);
    particles[i].color=vec4(mix(vec3(3.8,0.35,0.025),vec3(5.0,3.4,1.2),c)*alpha,alpha);
    particles[i].rotation=vec4(angle,0.0,0.0,0.0);
}
