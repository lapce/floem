

/// Filled circle.
const vgerCircle = 0;

/// Stroked arc.
const vgerArc = 1;

/// Rounded corner rectangle.
const vgerRect = 2;

/// Stroked rounded rectangle.
const vgerRectStroke = 3;

/// Single-segment quadratic bezier curve.
const vgerBezier = 4;

/// line segment
const vgerSegment = 5;

/// Multi-segment bezier curve.
const vgerCurve = 6;

/// Connection wire. See https://www.shadertoy.com/view/NdsXRl
const vgerWire = 7;

/// Text rendering.
const vgerGlyph = 8;

/// Text rendering.
const vgerColorGlyph = 9;

/// Path fills.
const vgerPathFill = 10;

/// Svg with override color
const overrideColorSvg = 11;

struct Prim {

    /// Min and max coordinates of the quad we're rendering.
    quad_bounds_min: vec2<f32>,
    quad_bounds_max: vec2<f32>,

    /// Index of transform applied to drawing region.
    xform: u32,

    /// Type of primitive.
    prim_type: u32,

    /// Stroke width.
    width: f32,

    /// Radius of circles. Corner radius for rounded rectangles.
    radius: f32,

    /// Control vertices.
    cv0: vec2<f32>,
    cv1: vec2<f32>,
    cv2: vec2<f32>,

    /// Start of the control vertices, if they're in a separate buffer.
    start: u32,

    /// Number of control vertices (vgerCurve and vgerPathFill)
    count: u32,

    /// Index of paint applied to drawing region.
    paint: u32,

    /// Glyph region index.
    glyph: u32,

    /// Min and max coordinates in texture space.
    tex_bounds_min: vec2<f32>,
    tex_bounds_max: vec2<f32>,

    /// Index of scissor rectangle.
    scissor: u32,

    /// Alignment padding.
    pad: u32,

};

fn proj(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return normalize(a) * dot(a,b) / length(a);
}

fn orth(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return b - proj(a, b);
}

fn rot90(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(-p.y, p.x);
}

// From https://www.iquilezles.org/www/articles/distfunctions2d/distfunctions2d.htm
// See also https://www.shadertoy.com/view/4dfXDn

fn sdCircle(p: vec2<f32>, r: f32) -> f32
{
    return length(p) - r;
}

fn sdBox(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32
{
    let d = abs(p)-b+r;
    return length(max(d,vec2<f32>(0.0, 0.0))) + min(max(d.x,d.y),0.0)-r;
}

fn sdSegment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width: f32) -> f32
{
    var dir = a-b;
    let lngth = length(dir);
    dir = dir / lngth;
    let proj = max(0.0, min(lngth, dot((a - p), dir))) * dir;
    return length( (a - p) - proj ) - (width / 2.0);
}

fn sdSegment2(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width: f32) -> f32
{
    let u = normalize(b-a);
    let v = rot90(u);

    var pp = p;
    pp = pp - (a+b)/2.0;
    pp = pp * mat2x2<f32>(u, v);
    return sdBox(pp, vec2<f32>(length(b-a)/2.0, width/2.0), 0.0);
}

// sca is {sin,cos} of orientation
// scb is {sin,cos} of aperture angle
fn sdArc(p: vec2<f32>, sca: vec2<f32>, scb: vec2<f32>, ra: f32, rb: f32 ) -> f32
{
    var pp = p * mat2x2<f32>(vec2<f32>(sca.x,sca.y),vec2<f32>(-sca.y,sca.x));
    pp.x = abs(pp.x);
    var k = 0.0;
    if (scb.y*pp.x>scb.x*pp.y) {
        k = dot(pp,scb);
    } else {
        k = length(pp);
    }
    return sqrt( dot(pp,pp) + ra*ra - 2.0*ra*k ) - rb;
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v,v);
}

fn sdBezier(pos: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32> ) -> f32
{
    let a = B - A;
    let b = A - 2.0*B + C;
    let c = a * 2.0;
    let d = A - pos;
    let kk = 1.0/dot(b,b);
    let kx = kk * dot(a,b);
    let ky = kk * (2.0*dot(a,a)+dot(d,b)) / 3.0;
    let kz = kk * dot(d,a);
    var res = 0.0;
    let p = ky - kx*kx;
    let p3 = p*p*p;
    let q = kx*(2.0*kx*kx + -3.0*ky) + kz;
    var h = q*q + 4.0*p3;
    if( h >= 0.0)
    {
        h = sqrt(h);
        let x = (vec2<f32>(h,-h)-q)/2.0;
        let uv = sign(x)*pow(abs(x), vec2<f32>(1.0/3.0));
        let t = clamp( uv.x+uv.y-kx, 0.0, 1.0 );
        res = dot2(d + (c + b*t)*t);
    }
    else
    {
        let z = sqrt(-p);
        let v = acos( q/(p*z*2.0) ) / 3.0;
        let m = cos(v);
        let n = sin(v)*1.732050808;
        let t = clamp(vec3<f32>(m+m,-n-m,n-m)*z-kx, vec3<f32>(0.0), vec3<f32>(1.0));
        res = min( dot2(d+(c+b*t.x)*t.x),
                   dot2(d+(c+b*t.y)*t.y) );
        // the third root cannot be the closest
        // res = min(res,dot2(d+(c+b*t.z)*t.z));
    }
    return sqrt( res );
}

fn sdSubtract(d1: f32, d2: f32) -> f32
{
    return max(-d1, d2);
}

fn sdPie(p: vec2<f32>, n: vec2<f32>) -> f32
{
    return abs(p).x * n.y + p.y*n.x;
}

/// Arc with square ends.
fn sdArc2(p: vec2<f32>, sca: vec2<f32>, scb: vec2<f32>, radius: f32, width: f32) -> f32
{
    // Rotate point.
    let pp = p * mat2x2<f32>(sca,vec2<f32>(-sca.y,sca.x));
    return sdSubtract(sdPie(pp, vec2<f32>(scb.x, -scb.y)),
                     abs(sdCircle(pp, radius)) - width);
}

// From https://www.shadertoy.com/view/4sySDK

fn inv(M: mat2x2<f32>) -> mat2x2<f32> {
    return (1.0 / determinant(M)) * mat2x2<f32>(
        vec2<f32>(M[1][1], -M[0][1]),
        vec2<f32>(-M[1][0], M[0][0]));
}

fn sdBezier2(uv: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {

    let trf1 = mat2x2<f32>( vec2<f32>(-1.0, 2.0), vec2<f32>(1.0, 2.0) );
    let trf2 = inv(mat2x2<f32>(p0-p1, p2-p1));
    let trf=trf1*trf2;

    let uv2 = uv - p1;
    var xy =trf*uv2;
    xy.y = xy.y - 1.0;

    var gradient: vec2<f32>;
    gradient.x=2.*trf[0][0]*(trf[0][0]*uv2.x+trf[1][0]*uv2.y)-trf[0][1];
    gradient.y=2.*trf[1][0]*(trf[0][0]*uv2.x+trf[1][0]*uv2.y)-trf[1][1];

    return (xy.x*xy.x-xy.y)/length(gradient);
}

fn det(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x*b.y-b.x*a.y; }

fn closestPointInSegment( a: vec2<f32>, b: vec2<f32>) -> vec2<f32>
{
    let ba = b - a;
    return a + ba*clamp( -dot(a,ba)/dot(ba,ba), 0.0, 1.0 );
}

// From: http://research.microsoft.com/en-us/um/people/hoppe/ravg.pdf
fn get_distance_vector(b0: vec2<f32>, b1: vec2<f32>, b2: vec2<f32>) -> vec2<f32> {

    let a=det(b0,b2);
    let b=2.0*det(b1,b0);
    let d=2.0*det(b2,b1);

    let f=b*d-a*a;
    let d21=b2-b1; let d10=b1-b0; let d20=b2-b0;
    var gf=2.0*(b*d21+d*d10+a*d20);
    gf=vec2<f32>(gf.y,-gf.x);
    let pp=-f*gf/dot(gf,gf);
    let d0p=b0-pp;
    let ap=det(d0p,d20); let bp=2.0*det(d10,d0p);
    // (note that 2*ap+bp+dp=2*a+b+d=4*area(b0,b1,b2))
    let t=clamp((ap+bp)/(2.0*a+b+d), 0.0 ,1.0);
    return mix(mix(b0,b1,t),mix(b1,b2,t),t);

}

fn sdBezierApprox(p: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {

    let v0 = normalize(B - A); let v1 = normalize(C - A);
    let det = v0.x * v1.y - v1.x * v0.y;
    if(abs(det) < 0.01) {
        return sdBezier(p, A, B, C);
    }

    return length(get_distance_vector(A-p, B-p, C-p));
}

fn sdBezierApprox2(p: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {
    return length(get_distance_vector(A-p, B-p, C-p));
}

struct BBox {
    min: vec2<f32>,
    max: vec2<f32>,
};

fn expand(box: BBox, p: vec2<f32>) -> BBox {
    var result: BBox;
    result.min = min(box.min, p);
    result.max = max(box.max, p);
    return result;
}

struct Prims {
    prims: array<Prim>,
};

@group(0)
@binding(0)
var<storage> prims: Prims;

struct CVS {
    cvs: array<vec2<f32>>,
};

@group(0)
@binding(1)
var<storage> cvs: CVS;

fn sdPrimBounds(prim: Prim) -> BBox {
    var b: BBox;
    switch (prim.prim_type) {
        case 0u: { // vgerCircle
            b.min = prim.cv0 - prim.radius;
            b.max = prim.cv0 + prim.radius;
        }
        case 1u: { // vgerArc
            b.min = prim.cv0 - prim.radius;
            b.max = prim.cv0 + prim.radius;
        }
        case 2u: { // vgerRect
            b.min = prim.cv0;
            b.max = prim.cv1;
        }
        case 3u: { // vgerRectStroke
            b.min = prim.cv0;
            b.max = prim.cv1;
        }
        case 4u: { // vgerBezier
            b.min = min(min(prim.cv0, prim.cv1), prim.cv2);
            b.max = max(max(prim.cv0, prim.cv1), prim.cv2);
        }
        case 5u: { // vgerSegment
            b.min = min(prim.cv0, prim.cv1);
            b.max = max(prim.cv0, prim.cv1);
        }
        case 6u: { // vgerCurve
            b.min = vec2<f32>(1e10, 1e10);
            b.max = -b.min;
            for(var i: i32 = 0; i < i32(prim.count * 3u); i = i+1) {
                b = expand(b, cvs.cvs[i32(prim.start)+i]);
            }
        }
        case 7u: { // vgerSegment
            b.min = min(prim.cv0, prim.cv1);
            b.max = max(prim.cv0, prim.cv1);
        }
        case 8u: { // vgerGlyph
            b.min = prim.cv0;
            b.max = prim.cv1;
        }
        case 9u: { // vgerColorGlyph
            b.min = prim.cv0;
            b.max = prim.cv1;
        }
        case 10u: { // vgerPathFill
            b.min = vec2<f32>(1e10, 1e10);
            b.max = -b.min;
            for(var i: i32 = 0; i < i32(prim.count * 3u); i = i+1) {
                b = expand(b, cvs.cvs[i32(prim.start)+i]);
            }
        }
        case 11u: { // overrideColorSvg
            b.min = prim.cv0;
            b.max = prim.cv1;
        }
        default: {}
    }
    return b;
}

fn lineTest(p: vec2<f32>, A: vec2<f32>, B: vec2<f32>) -> bool {

    let cs = i32(A.y < p.y) * 2 + i32(B.y < p.y);

    if(cs == 0 || cs == 3) { return false; } // trivial reject

    let v = B - A;

    // Intersect line with x axis.
    let t = (p.y-A.y)/v.y;

    return (A.x + t*v.x) > p.x;

}

/// Is the point with the area between the curve and line segment A C?
fn bezierTest(p: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> bool {

    // Compute barycentric coordinates of p.
    // p = s * A + t * B + (1-s-t) * C
    let v0 = B - A; let v1 = C - A; let v2 = p - A;
    let det = v0.x * v1.y - v1.x * v0.y;
    let s = (v2.x * v1.y - v1.x * v2.y) / det;
    let t = (v0.x * v2.y - v2.x * v0.y) / det;

    if(s < 0.0 || t < 0.0 || (1.0-s-t) < 0.0) {
        return false; // outside triangle
    }

    // Transform to canonical coordinte space.
    let u = s * 0.5 + t;
    let v = t;

    return u*u < v;

}

fn sdPrim(prim: Prim, p: vec2<f32>, filterWidth: f32) -> f32 {
    var d = 1e10;
    var s = 1.0;
    switch(prim.prim_type) {
        case 0u: { // vgerCircle
            d = sdCircle(p - prim.cv0, prim.radius);
        }
        case 1u: { // vgerArc
            d = sdArc2(p - prim.cv0, prim.cv1, prim.cv2, prim.radius, prim.width/2.0);
        }
        case 2u: { // vgerRect
            let center = 0.5*(prim.cv1 + prim.cv0);
            let size = prim.cv1 - prim.cv0;
            d = sdBox(p - center, 0.5*size, prim.radius);
        }
        case 3u: { // vgerRectStroke
            let center = 0.5*(prim.cv1 + prim.cv0);
            let size = prim.cv1 - prim.cv0;
            d = abs(sdBox(p - center, 0.5*size, prim.radius)) - prim.width/2.0;
        }
        case 4u: { // vgerBezier
            d = sdBezierApprox(p, prim.cv0, prim.cv1, prim.cv2) - prim.width/2.0;
        }
        case 5u: { // vgerSegment
            d = sdSegment2(p, prim.cv0, prim.cv1, prim.width);
        }
        case 6u: { // vgerCurve
            for(var i=0; i<i32(prim.count); i = i+1) {
                let j = i32(prim.start) + 3*i;
                d = min(d, sdBezierApprox(p, cvs.cvs[j], cvs.cvs[j+1], cvs.cvs[j+2]));
            }
        }
        case 7u: { // vgerSegment
            d = sdSegment2(p, prim.cv0, prim.cv1, prim.width);
        }
        case 8u: { // vgerGlyph
            let center = 0.5*(prim.cv1 + prim.cv0);
            let size = prim.cv1 - prim.cv0;
            d = sdBox(p - center, 0.5*size, prim.radius);
        }
        case 9u: { // vgerColorGlyph
            let center = 0.5*(prim.cv1 + prim.cv0);
            let size = prim.cv1 - prim.cv0;
            d = sdBox(p - center, 0.5*size, prim.radius);
        }
        case 10u: { // vgerPathFill
            for(var i=0; i<i32(prim.count); i = i+1) {
                let j = i32(prim.start) + 3*i;
                let a = cvs.cvs[j];
                let b = cvs.cvs[j+1];
                let c = cvs.cvs[j+2];

                var skip = false;
                let xmax = p.x + filterWidth;
                let xmin = p.x - filterWidth;

                // If the hull is far enough away, don't bother with
                // a sdf.
                if(a.x > xmax && b.x > xmax && c.x > xmax) {
                    skip = true;
                } else if(a.x < xmin && b.x < xmin && c.x < xmin) {
                    skip = true;
                }

                if(!skip) {
                    d = min(d, sdBezier(p, a, b, c));
                }

                if(lineTest(p, a, c)) {
                    s = -s;
                }

                // Flip if inside area between curve and line.
                if(!skip) {
                    if(bezierTest(p, a, b, c)) {
                        s = -s;
                    }
                }

            }
            d = d * s;
            break;
        }
        case 11u: { // overrideColorSvg
            let center = 0.5*(prim.cv1 + prim.cv0);
            let size = prim.cv1 - prim.cv0;
            d = sdBox(p - center, 0.5*size, prim.radius);
        }
        default: { }
    }
    return d;
}

struct XForms {
    xforms: array<mat4x4<f32>>,
};

@group(0)
@binding(2)
var<storage> xforms: XForms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) prim_index: u32,

    /// Texture space point.
    @location(1) t: vec2<f32>,

    /// Point transformed by current transform.
    @location(2) p: vec2<f32>,

    /// Screen size.
    @location(3) size: vec2<f32>,
};

struct Uniforms {
    size: vec2<f32>,
    atlas_size: vec2<f32>,
};

@group(1)
@binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) instance: u32
) -> VertexOutput {
    var out: VertexOutput;
    out.prim_index = instance;

    let prim = prims.prims[instance];

    var q: vec2<f32>;
    switch(vid) {
        case 0u: {
            q = prim.quad_bounds_min;
            //q = vec3<f32>(80.0, 80.0, 1.0);
            out.t = prim.tex_bounds_min;
        }
        case 1u: {
            q = vec2<f32>(prim.quad_bounds_min.x, prim.quad_bounds_max.y);
            //q = vec3<f32>(80.0,120.0, 1.0);
            out.t = vec2<f32>(prim.tex_bounds_min.x, prim.tex_bounds_max.y);
        }
        case 2u: {
            q = vec2<f32>(prim.quad_bounds_max.x, prim.quad_bounds_min.y);
            //q = vec3<f32>(120.0,80.0, 1.0);
            out.t = vec2<f32>(prim.tex_bounds_max.x, prim.tex_bounds_min.y);
        }
        case 3u: {
            q = prim.quad_bounds_max;
            //q = vec3<f32>(120.0,120.0, 1.0);
            out.t = prim.tex_bounds_max;
        }
        default: { }
    }

    out.p = (xforms.xforms[prim.xform] * vec4<f32>(q, 0.0, 1.0)).xy;
    out.position = vec4<f32>((2.0 * out.p / uniforms.size - 1.0) * vec2<f32>(1.0, -1.0), 0.0, 1.0);
    out.size = uniforms.atlas_size;

    return out;
}

struct PackedMat3x2 {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    m31: f32,
    m32: f32,
};

fn unpack_mat3x2(m: PackedMat3x2) -> mat3x2<f32> {
    return mat3x2<f32>(m.m11, m.m12, m.m21, m.m22, m.m31, m.m32);
}

struct Paint {              // align  size
    xform: PackedMat3x2,    // 8      24
    glow: f32,              // 4      4
    image: i32,             // 4      4
    inner_color: vec4<f32>, // 16     16
    outer_color: vec4<f32>, // 16     16
};

struct Paints {
    paints: array<Paint>,
};

@group(0)
@binding(3)
var<storage> paints: Paints;

fn apply(paint: Paint, p: vec2<f32>) -> vec4<f32> {
    let local_point = unpack_mat3x2(paint.xform) * vec3<f32>(p, 1.0);
    let d = clamp(local_point, vec2<f32>(0.0), vec2<f32>(1.0)).x;

    return mix(paint.inner_color, paint.outer_color, d);
}

struct Scissor {
    xform: PackedMat3x2,
    origin: vec2<f32>,
    size: vec2<f32>,
    radius: f32,
    pad: f32,
};

struct Scissors {
    scissors: array<Scissor>,
};

@group(0)
@binding(4)
var<storage> scissors: Scissors;

fn scissor_mask(scissor: Scissor, p: vec2<f32>) -> f32 {
    let M = unpack_mat3x2(scissor.xform);
    let pp = (M * vec3<f32>(p, 1.0)).xy;
    let center = scissor.origin + 0.5 * scissor.size;
    let size = scissor.size;
    if sdBox(pp - center, 0.5 * size, scissor.radius) < 0.0 {
        return 1.0;
    } else {
        return 0.0;
    }
}

@group(1)
@binding(1)
var samp : sampler;

@group(1)
@binding(2)
var color_samp : sampler;

@group(2)
@binding(0)
var glyph_atlas: texture_2d<f32>;

@group(2)
@binding(1)
var color_atlas: texture_2d<f32>;


// sRGB to linear conversion for one channel.
fn toLinear(s: f32) -> f32
{
    if s < 0.04045 {
        return s/12.92;
    }
    return pow((s + 0.055)/1.055, 2.4);
}

// This approximates the error function, needed for the gaussian integral
fn erf(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var y = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    y *= y;
    return s - s / (y * y);
}

fn gaussian(x: f32, sigma: f32) -> f32 {
  let pi: f32 = 3.141592653589793;
  return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * pi) * sigma);
}

fn roundedBoxShadowX(x: f32, y: f32, sigma: f32, corner: f32, halfSize: vec2<f32>) -> f32 {
    let delta = min(halfSize.y - corner - abs(y), 0.0);
    let curved = halfSize.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf((x + vec2(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

@fragment
fn fs_main(
    in: VertexOutput,
) -> @location(0) vec4<f32> {

    let fw = length(fwidth(in.t));
    let prim = prims.prims[in.prim_index];
    let paint = paints.paints[prim.paint];
    let scissor = scissors.scissors[prim.scissor];

    // Look up glyph alpha (if not a glyph, still have to because of wgsl).
    // let a = textureSample(glyph_atlas, samp, (in.t+0.5)/1024.0).r;
    // let mask = textureLoad(glyph_atlas, vec2<i32>(in.t), 0);
    let mask = textureSample(glyph_atlas, samp, in.t/in.size);
    let color_mask = textureSample(color_atlas, color_samp, in.t/in.size);

    // Look up image color (if no active image, still have to because of wgsl).
    // Note that we could use a separate shader if that's a perf hit.
    let t = unpack_mat3x2(paint.xform) * vec3<f32>(in.t, 1.0);
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    let s = scissor_mask(scissor, in.p);

    if(prim.prim_type == 2u && prim.cv2.x > 0.0) {
        let p = in.t;
        let blur_radius = prim.cv2.x;
        let center = 0.5*(prim.cv1 + prim.cv0);
        let half_size = 0.5*(prim.cv1 - prim.cv0);
        let point = p - center;

        let low = point.y - half_size.y;
        let high = point.y + half_size.y;
        let start = clamp(-3.0 * blur_radius, low, high);
        let end = clamp(3.0 * blur_radius, low, high);

        let step = (end - start) / 4.0;
        var y = start + step * 0.5;
        var value = 0.0;
        for (var i: i32 = 0; i < 4; i++) {
            value += roundedBoxShadowX(point.x, point.y - y, blur_radius, prim.radius, half_size) * gaussian(y, blur_radius) * step;
            y += step;
        }

        return s * vec4<f32>(paint.inner_color.rgb, value * paint.inner_color.a);
    }

    if(prim.prim_type == 8u) { // vgerGlyph
        if (mask.r <= 0.0) {
            discard;
        }

        let c = paint.inner_color;

        // XXX: using toLinear is a bit of a guess. Gets us closer
        // to matching the glyph atlas in the output.
        var color = vec4<f32>(c.rgb, c.a * mask.r);

        //if(glow) {
        //    color.a *= paint.glow;
        //}

        return s * color;
    }

    if(prim.prim_type == 9u) { // vgerColorGlyph

        let c = paint.inner_color;

        // XXX: using toLinear is a bit of a guess. Gets us closer
        // to matching the glyph atlas in the output.
        var color = vec4<f32>(color_mask.rgb, c.a * color_mask.a);

        //if(glow) {
        //    color.a *= paint.glow;
        //}

        return s * color;
    }

    if(prim.prim_type == 11u) { // overrideColorSvg

        let c = paint.inner_color;

        // XXX: using toLinear is a bit of a guess. Gets us closer
        // to matching the glyph atlas in the output.
        var color = vec4<f32>(c.rgb, c.a * color_mask.a);

        //if(glow) {
        //    color.a *= paint.glow;
        //}

        return s * color;
    }

    let d = sdPrim(prim, in.t, fw);
    if paint.image == -1 {
        color = apply(paint, in.t);
    }

    return s * mix(vec4<f32>(color.rgb,0.0), color, 1.0-smoothstep(-fw/2.0,fw/2.0,d) );
}
