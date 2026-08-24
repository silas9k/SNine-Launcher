// SNine Launcher WebGL player renderer.
// Player geometry is based on the SNine NRC Studio mapping; SNine cosmetics are
// attached to the same biped bones and loaded from the client's runtime catalog.
// @ts-nocheck
const D2R = Math.PI / 180;
const clamp = (x: number, a: number, b: number) => Math.max(a, Math.min(b, x));
const vec3 = (value: unknown, fallback = [0, 0, 0]) => Array.isArray(value) && value.length >= 3
  ? [Number(value[0]) || 0, Number(value[1]) || 0, Number(value[2]) || 0]
  : fallback.slice();

function m4() {
  return new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
}
function mm(a: Float32Array, b: Float32Array) {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c += 1) for (let r = 0; r < 4; r += 1) {
    let s = 0;
    for (let k = 0; k < 4; k += 1) s += a[k * 4 + r] * b[c * 4 + k];
    o[c * 4 + r] = s;
  }
  return o;
}
function mt(x: number, y: number, z: number) { const o = m4(); o[12] = x; o[13] = y; o[14] = z; return o; }
function ms(x: number, y: number, z: number) { const o = m4(); o[0] = x; o[5] = y; o[10] = z; return o; }
function qEulerXYZ(x: number, y: number, z: number) {
  x *= D2R / 2; y *= D2R / 2; z *= D2R / 2;
  const c1 = Math.cos(x), c2 = Math.cos(y), c3 = Math.cos(z), s1 = Math.sin(x), s2 = Math.sin(y), s3 = Math.sin(z);
  return [s1*c2*c3+c1*s2*s3, c1*s2*c3-s1*c2*s3, c1*c2*s3+s1*s2*c3, c1*c2*c3-s1*s2*s3];
}
function mq(q: number[]) {
  const [x, y, z, w] = q, x2 = x + x, y2 = y + y, z2 = z + z, xx = x*x2, xy = x*y2, xz = x*z2, yy = y*y2, yz = y*z2, zz = z*z2, wx = w*x2, wy = w*y2, wz = w*z2;
  return new Float32Array([1-(yy+zz), xy+wz, xz-wy, 0, xy-wz, 1-(xx+zz), yz+wx, 0, xz+wy, yz-wx, 1-(xx+yy), 0, 0, 0, 0, 1]);
}
function mtrs(p: number[], r: number[], s: number[]) { return mm(mm(mt(p[0], p[1], p[2]), mq(qEulerXYZ(r[0], r[1], r[2]))), ms(s[0], s[1], s[2])); }
function perspective(fov: number, aspect: number, n: number, f: number) {
  const t = 1 / Math.tan(fov / 2), o = new Float32Array(16);
  o[0] = t / aspect; o[5] = t; o[10] = (f + n) / (n - f); o[11] = -1; o[14] = 2 * f * n / (n - f);
  return o;
}
function lookAt(e: number[], c: number[], u: number[]) {
  let zx=e[0]-c[0], zy=e[1]-c[1], zz=e[2]-c[2], l=Math.hypot(zx,zy,zz); zx/=l; zy/=l; zz/=l;
  let xx=u[1]*zz-u[2]*zy, xy=u[2]*zx-u[0]*zz, xz=u[0]*zy-u[1]*zx; l=Math.hypot(xx,xy,xz); xx/=l; xy/=l; xz/=l;
  const yx=zy*xz-zz*xy, yy=zz*xx-zx*xz, yz=zx*xy-zy*xx, o=m4();
  o[0]=xx; o[1]=yx; o[2]=zx; o[4]=xy; o[5]=yy; o[6]=zy; o[8]=xz; o[9]=yz; o[10]=zz;
  o[12]=-(xx*e[0]+xy*e[1]+xz*e[2]); o[13]=-(yx*e[0]+yy*e[1]+yz*e[2]); o[14]=-(zx*e[0]+zy*e[1]+zz*e[2]);
  return o;
}

class Bone {
  name: string; pivot: number[]; parent: Bone | null; staticRot: number[]; children: Bone[] = []; drawables: any[] = []; pos=[0,0,0]; rot=[0,0,0]; scale=[1,1,1]; world=m4();
  constructor(name: string, pivot=[0,0,0], parent: Bone | null=null, staticRot=[0,0,0]) { this.name=name; this.pivot=pivot.slice(); this.parent=parent; this.staticRot=staticRot.slice(); if (parent) parent.children.push(this); }
  update(parentWorld=m4()) { const pp=this.parent?this.parent.pivot:[0,0,0], bind=[this.pivot[0]-pp[0],this.pivot[1]-pp[1],this.pivot[2]-pp[2]], p=[bind[0]+this.pos[0],bind[1]+this.pos[1],bind[2]+this.pos[2]], r=[this.staticRot[0]+this.rot[0],this.staticRot[1]+this.rot[1],this.staticRot[2]+this.rot[2]]; this.world=mm(parentWorld,mtrs(p,r,this.scale)); for (const child of this.children) child.update(this.world); }
}

export interface RendererCosmeticAsset {
  id: string;
  kind: string;
  name: string;
  model: unknown | null;
  definition?: Record<string, unknown>;
}
export interface LoadedRendererCosmetic {
  asset: RendererCosmeticAsset;
  image: HTMLImageElement | null;
}

export class LauncherSkinRenderer {
  canvas: HTMLCanvasElement; gl: WebGLRenderingContext; program: any; textures = new Set<WebGLTexture>(); skinTex: any = null; bones = new Map<string, Bone>(); topBones: Bone[] = []; cameraYaw=0; cameraPitch=1.5; cameraDistance=62; target=[0,16,0]; homeCameraYaw=0; homeCameraPitch=1.5; homeCameraDistance=62; homeTarget=[0,16,0]; drag: [number,number] | null=null; running=true; model: "slim"|"classic"="slim"; frameHandle=0; reducedMotion=false;
  cosmeticSources: LoadedRendererCosmetic[] = []; cosmeticTextures = new Set<WebGLTexture>(); cosmeticRoots: Array<{ bone: Bone; kind: string; phase: number }> = []; glintEnabled=false; glintStyle=1; renderTime=0;

  constructor(canvas: HTMLCanvasElement, reducedMotion=false) {
    this.canvas = canvas; this.reducedMotion = reducedMotion;
    const gl = canvas.getContext("webgl", { alpha: true, antialias: true, premultipliedAlpha: false }) as WebGLRenderingContext | null;
    if (!gl) throw new Error("WebGL unavailable");
    this.gl = gl; this.program = this.makeProgram(); this.buildPlayer("slim"); this.attachOrbit(); this.frame = this.frame.bind(this); this.frameHandle = requestAnimationFrame(this.frame);
  }
  sh(type: number, src: string) { const g=this.gl,s=g.createShader(type)!; g.shaderSource(s,src); g.compileShader(s); if(!g.getShaderParameter(s,g.COMPILE_STATUS)) throw new Error(g.getShaderInfoLog(s) || "shader compile failed"); return s; }
  makeProgram() {
    const g=this.gl;
    const vs=`
attribute vec3 aP;
attribute vec2 aU;
attribute vec3 aN;
uniform mat4 uMVP;
uniform mat4 uM;
varying vec2 vU;
varying vec3 vN;
varying vec3 vP;
void main(){
  vU=aU;
  vN=normalize(mat3(uM)*aN);
  vP=(uM*vec4(aP,1.0)).xyz;
  gl_Position=uMVP*vec4(aP,1.0);
}`;
    const fs=`
precision mediump float;
varying vec2 vU;
varying vec3 vN;
varying vec3 vP;
uniform sampler2D uT;
uniform float uAlpha;
uniform float uGlintPass;
uniform float uGlintStyle;
uniform float uTime;

#define PI 3.14159265358979323846

float hash21(vec2 p){
  p=fract(p*vec2(123.34,456.21));
  p+=dot(p,p+45.32);
  return fract(p.x*p.y);
}
float noise2(vec2 p){
  vec2 i=floor(p),f=fract(p);
  f=f*f*(3.0-2.0*f);
  float a=hash21(i),b=hash21(i+vec2(1.0,0.0)),c=hash21(i+vec2(0.0,1.0)),d=hash21(i+vec2(1.0,1.0));
  return mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
}
vec2 rot(vec2 p,float a){float c=cos(a),s=sin(a);return mat2(c,-s,s,c)*p;}
float softBand(float x,float width){float d=abs(fract(x)-0.5);return smoothstep(width,0.0,d);}

// Minecraft's armor/entity glint texture transform: small repeated texture,
// ~10 degree rotation and two different scroll periods. Using world/model
// coordinates here avoids stretching the glint across the 64x64 skin atlas.
vec2 armorGlintUv(){
  vec2 p=vec2(vP.x+vP.z*0.72,vP.y);
  p=rot(p*0.16,0.1745329252);
  p+=vec2(-fract(uTime/13.75),fract(uTime/3.75));
  return p;
}

float vanillaGlintMask(vec2 uv){
  // Dense repeated enchanted-armor streaks, not the previous giant scanlines.
  float a=softBand(uv.x+uv.y*0.66,0.29);
  float b=softBand(uv.x*0.63-uv.y*0.92+0.31,0.38)*0.48;
  float c=softBand(uv.x*1.73+uv.y*1.08+0.12,0.44)*0.23;
  float grain=0.74+noise2(uv*18.0)*0.26;
  return clamp((a*0.82+b+c)*grain,0.0,1.0);
}

vec3 auroraPalette(float t){
  t=fract(t)*12.0;
  float i=floor(t),f=fract(t);
  vec3 a,b;
  if(i<1.0){a=vec3(0.020,0.165,0.169);b=vec3(0.031,0.478,0.404);}
  else if(i<2.0){a=vec3(0.031,0.478,0.404);b=vec3(0.071,0.788,0.541);}
  else if(i<3.0){a=vec3(0.071,0.788,0.541);b=vec3(0.357,1.000,0.761);}
  else if(i<4.0){a=vec3(0.357,1.000,0.761);b=vec3(0.769,1.000,0.914);}
  else if(i<5.0){a=vec3(0.769,1.000,0.914);b=vec3(0.388,0.945,1.000);}
  else if(i<6.0){a=vec3(0.388,0.945,1.000);b=vec3(0.224,0.545,1.000);}
  else if(i<7.0){a=vec3(0.224,0.545,1.000);b=vec3(0.412,0.361,1.000);}
  else if(i<8.0){a=vec3(0.412,0.361,1.000);b=vec3(0.706,0.392,1.000);}
  else if(i<9.0){a=vec3(0.706,0.392,1.000);b=vec3(0.941,0.427,1.000);}
  else if(i<10.0){a=vec3(0.941,0.427,1.000);b=vec3(1.000,0.569,0.784);}
  else if(i<11.0){a=vec3(1.000,0.569,0.784);b=vec3(0.243,0.910,0.753);}
  else {a=vec3(0.243,0.910,0.753);b=vec3(0.020,0.165,0.169);}
  return mix(a,b,smoothstep(0.0,1.0,f));
}

vec4 minecraftGlint(float style,float skinAlpha){
  vec2 uv=armorGlintUv();
  float base=vanillaGlintMask(uv);

  // 1 = vanilla enchantment glint (entityGlint)
  if(style<1.5){
    vec3 purple=mix(vec3(0.26,0.08,0.48),vec3(0.78,0.34,1.0),base);
    return vec4(purple*base,skinAlpha);
  }

  // 2 = SNine Aurora. This follows the client texture manager's warped
  // emerald/mint/cyan/violet/rose curtains while the vanilla glint transform
  // supplies the scrolling movement.
  if(style<2.5){
    vec2 q=fract(uv);
    float large=noise2(q*4.0+vec2(1.13,2.71));
    float medium=noise2(q*9.0+vec2(3.47,7.19));
    float fine=noise2(q*21.0+vec2(7.19,1.13));
    float hw=(large-0.5)*0.30+(medium-0.5)*0.12;
    float vw=(medium-0.5)*0.22+(fine-0.5)*0.045;
    float cc=q.x*1.12+q.y*0.66+hw+sin((q.y*2.0+medium*0.7)*PI*2.0)*0.055;
    float cw=0.5+0.5*sin(cc*PI*4.8);
    float sc=q.x*-0.48+q.y*1.18+vw+large*0.17;
    float sw=0.5+0.5*sin(sc*PI*3.3+1.15);
    float broad=pow(cw,0.72),secondary=pow(sw,1.18);
    float ridgeA=pow(clamp(1.0-abs(cw*2.0-1.0),0.0,1.0),5.4);
    float ridgeB=pow(clamp(1.0-abs(sw*2.0-1.0),0.0,1.0),7.0);
    float coverage=smoothstep(0.20,0.82,broad*0.56+secondary*0.23+large*0.17+medium*0.12);
    float strength=clamp(base*(0.72+coverage*1.05)+ridgeA*0.21+ridgeB*0.11,0.0,1.0);
    float phase=fract(q.x*0.54+q.y*0.39+hw*0.92+sw*0.11+fine*0.055);
    vec3 col=auroraPalette(phase);
    float power=0.035+coverage*0.105+strength*0.81;
    vec3 outc=vec3(0.006,0.012,0.018)+col*power;
    float edge=clamp(ridgeA*0.54+ridgeB*0.27,0.0,0.68);
    outc=mix(outc,vec3(0.76,1.0,0.93),edge);
    float sparkle=pow(noise2(q*46.0+vec2(4.0,9.0)),18.0)*0.70;
    outc+=vec3(0.74,1.0,0.95)*sparkle;
    return vec4(outc,skinAlpha);
  }

  // 3 = rich gold glint: vanilla streak mask + warm gold wash + tiny highlights.
  if(style<3.5){
    vec2 q=fract(uv);
    float broad=0.82+(0.5+0.5*sin((q.x*3.10-q.y*2.25+0.08)*PI*2.0))*0.18;
    float fine=0.89+(0.5+0.5*sin((q.x*7.30+q.y*4.80+0.31)*PI*2.0))*0.11;
    float strength=clamp(base*1.72*broad*fine,0.0,1.0);
    float spark=pow(noise2(q*28.0+vec2(1.37,7.19)),20.0);
    vec3 gold=vec3(0.027,0.012,0.004)+vec3(0.90,0.55,0.11)*strength;
    gold+=vec3(1.0,0.90,0.45)*spark*0.85;
    return vec4(gold,skinAlpha);
  }

  // 4 = Shadow energySwirl: client speed age*.005 / age*.014.
  if(style<4.5){
    vec2 q=vec2(vP.x+vP.z*0.7,vP.y)*0.075+vec2(uTime*0.10,uTime*0.28);
    float sw=softBand(q.x*0.85-q.y*1.24,0.40);
    float smoke=noise2(q*3.7)*0.60+noise2(q*9.0)*0.25;
    float m=clamp(sw*0.65+smoke*0.40,0.0,1.0);
    return vec4(mix(vec3(0.018,0.012,0.025),vec3(0.36,0.17,0.53),m)*m,skinAlpha);
  }

  // 5 = Creeper energySwirl: age*.006 / age*.013.
  if(style<5.5){
    vec2 q=vec2(vP.x+vP.z*0.7,vP.y)*0.072+vec2(uTime*0.12,uTime*0.26);
    float cells=step(0.54,noise2(floor(q*8.0)));
    float pulse=0.45+0.55*softBand(q.x-q.y*0.72,0.43);
    vec3 green=mix(vec3(0.015,0.13,0.025),vec3(0.25,0.95,0.31),cells*pulse);
    return vec4(green*(0.35+cells*0.65),skinAlpha);
  }

  // 6 = Rainbow body glint. The client generates a vivid diagonal hue sweep
  // and then runs it through energySwirl at age*.010 / age*.018.
  if(style<6.5){
    vec2 q=vec2(vP.x+vP.z*0.7,vP.y)*0.070+vec2(uTime*0.20,uTime*0.36);
    float hue=fract(q.x*0.70+q.y*0.70+uTime*0.004);
    vec3 rgb=0.55+0.45*cos(2.0*PI*(hue+vec3(0.0,0.333,0.667)));
    return vec4(rgb*0.42,skinAlpha);
  }

  // 7 = Wither energySwirl: age*.010 / age*.010.
  vec2 q=vec2(vP.x+vP.z*0.7,vP.y)*0.075+vec2(uTime*0.20,uTime*0.20);
  float n=noise2(q*5.2);
  float lines=softBand(q.x+q.y*0.43,0.38);
  float m=clamp(n*0.58+lines*0.50,0.0,1.0);
  return vec4(vec3(0.07,0.075,0.085)*m+vec3(0.22,0.23,0.25)*pow(m,3.0),skinAlpha);
}

void main(){
  vec4 c=texture2D(uT,vU);
  if(c.a<uAlpha)discard;
  vec3 n=normalize(vN);
  float ax=abs(n.x),ay=abs(n.y),az=abs(n.z);
  float shade;
  if(ay>=ax&&ay>=az)shade=n.y>0.0?.98:.58;
  else if(ax>=az)shade=.72;
  else shade=.86;
  if(uGlintPass<.5){
    gl_FragColor=vec4(c.rgb*shade,c.a);
    return;
  }
  vec4 g=minecraftGlint(uGlintStyle,c.a);
  gl_FragColor=vec4(g.rgb*shade,g.a);
}`;
    const p=g.createProgram()!; g.attachShader(p,this.sh(g.VERTEX_SHADER,vs)); g.attachShader(p,this.sh(g.FRAGMENT_SHADER,fs)); g.linkProgram(p); if(!g.getProgramParameter(p,g.LINK_STATUS)) throw new Error(g.getProgramInfoLog(p) || "program link failed");
    return { p, aP:g.getAttribLocation(p,"aP"), aU:g.getAttribLocation(p,"aU"), aN:g.getAttribLocation(p,"aN"), uMVP:g.getUniformLocation(p,"uMVP"), uM:g.getUniformLocation(p,"uM"), uT:g.getUniformLocation(p,"uT"), uAlpha:g.getUniformLocation(p,"uAlpha"), uGlintPass:g.getUniformLocation(p,"uGlintPass"), uGlintStyle:g.getUniformLocation(p,"uGlintStyle"), uTime:g.getUniformLocation(p,"uTime") };
  }
  textureFromImage(img: HTMLImageElement, cosmetic=false, logicalSize?: number[]) { const g=this.gl,t=g.createTexture()!; g.bindTexture(g.TEXTURE_2D,t); g.pixelStorei(g.UNPACK_FLIP_Y_WEBGL,true); g.pixelStorei(g.UNPACK_PREMULTIPLY_ALPHA_WEBGL,false); g.texImage2D(g.TEXTURE_2D,0,g.RGBA,g.RGBA,g.UNSIGNED_BYTE,img); g.texParameteri(g.TEXTURE_2D,g.TEXTURE_MIN_FILTER,g.NEAREST); g.texParameteri(g.TEXTURE_2D,g.TEXTURE_MAG_FILTER,g.NEAREST); g.texParameteri(g.TEXTURE_2D,g.TEXTURE_WRAP_S,g.CLAMP_TO_EDGE); g.texParameteri(g.TEXTURE_2D,g.TEXTURE_WRAP_T,g.CLAMP_TO_EDGE); this.textures.add(t); if(cosmetic)this.cosmeticTextures.add(t); const uvW=Number(logicalSize?.[0])>0?Number(logicalSize?.[0]):img.width,uvH=Number(logicalSize?.[1])>0?Number(logicalSize?.[1]):img.height; return {gl:t,w:uvW,h:uvH,imageW:img.width,imageH:img.height}; }
  setSkin(img: HTMLImageElement, model: "slim"|"classic"="slim") { if(this.skinTex) { this.gl.deleteTexture(this.skinTex.gl); this.textures.delete(this.skinTex.gl); } this.skinTex=this.textureFromImage(img); this.buildPlayer(model); }
  setCosmetics(items: LoadedRendererCosmetic[]) {
    // Launcher preview intentionally does not render SNine glint cosmetics.
    // Glints stay an in-game effect only.
    this.cosmeticSources = items.filter((item) => {
      if (!item?.asset) return false;
      const kind = String(item.asset.kind || "").trim().toLowerCase();
      const id = String(item.asset.id || "").trim().toLowerCase();
      return kind !== "glint" && !id.includes("glint");
    });
    this.buildPlayer(this.model);
  }
  setReducedMotion(value: boolean) { this.reducedMotion = value; }
  setCameraPreset(yaw=0,pitch=1.5,distance=62,targetY=16){this.homeCameraYaw=yaw;this.homeCameraPitch=pitch;this.homeCameraDistance=distance;this.homeTarget=[0,targetY,0];this.cameraYaw=yaw;this.cameraPitch=pitch;this.cameraDistance=distance;this.target=[0,targetY,0];}
  attachOrbit() {
    const c=this.canvas;
    c.addEventListener("pointerdown",e=>{this.drag=[e.clientX,e.clientY];c.setPointerCapture?.(e.pointerId)});
    c.addEventListener("pointermove",e=>{if(!this.drag)return;this.cameraYaw+=(e.clientX-this.drag[0])*.35;this.cameraPitch=clamp(this.cameraPitch+(e.clientY-this.drag[1])*.25,-35,30);this.drag=[e.clientX,e.clientY]});
    c.addEventListener("pointerup",()=>this.drag=null); c.addEventListener("pointercancel",()=>this.drag=null); c.addEventListener("dblclick",()=>{this.cameraYaw=this.homeCameraYaw;this.cameraPitch=this.homeCameraPitch;this.cameraDistance=this.homeCameraDistance;this.target=[...this.homeTarget]}); c.addEventListener("wheel",e=>{e.preventDefault();this.cameraDistance=clamp(this.cameraDistance+Math.sign(e.deltaY)*3,48,96)},{passive:false});
  }
  clearCosmeticTextures(){for(const t of this.cosmeticTextures){this.gl.deleteTexture(t);this.textures.delete(t)}this.cosmeticTextures.clear();this.cosmeticRoots=[];this.glintEnabled=false;this.glintStyle=1;}
  clearModel(){this.bones.clear();this.topBones=[];}
  addBone(name:string,pivot:number[],parent:Bone|null=null,rot:number[]=[0,0,0]){const b=new Bone(name,pivot,parent,rot);this.bones.set(name,b);if(!parent)this.topBones.push(b);return b;}
  buildPlayer(model: "slim"|"classic"="slim") {
    this.model=model; this.clearCosmeticTextures(); this.clearModel(); const rig=this.addBone("bipedRig",[0,0,0]); const head=this.addBone("bipedHead",[0,24,0],rig),body=this.addBone("bipedBody",[0,24,0],rig),ra=this.addBone("bipedRightArm",[5,22,0],rig),la=this.addBone("bipedLeftArm",[-5,22,0],rig),rl=this.addBone("bipedRightLeg",[2,12,0],rig),ll=this.addBone("bipedLeftLeg",[-2,12,0],rig); const aw=model==="slim"?3:4;
    this.addPlayerBox(head,[-4,24,-4],[8,8,8],[0,0],0); this.addPlayerBox(head,[-4,24,-4],[8,8,8],[32,0],.5,true);
    this.addPlayerBox(body,[-4,12,-2],[8,12,4],[16,16],0); this.addPlayerBox(body,[-4,12,-2],[8,12,4],[16,32],.25,true);
    const rX=4,lX=model==="slim"?-7:-8;
    this.addPlayerBox(ra,[rX,12,-2],[aw,12,4],[40,16],0); this.addPlayerBox(ra,[rX,12,-2],[aw,12,4],[40,32],.25,true);
    this.addPlayerBox(la,[lX,12,-2],[aw,12,4],[32,48],0); this.addPlayerBox(la,[lX,12,-2],[aw,12,4],[48,48],.25,true);
    this.addPlayerBox(rl,[0,0,-2],[4,12,4],[0,16],0); this.addPlayerBox(rl,[0,0,-2],[4,12,4],[0,32],.25,true);
    this.addPlayerBox(ll,[-4,0,-2],[4,12,4],[16,48],0); this.addPlayerBox(ll,[-4,0,-2],[4,12,4],[0,48],.25,true);
    this.buildCosmetics();
  }
  addPlayerBox(bone:Bone,origin:number[],size:number[],uv:number[],inflate=0,_outer=false){if(!this.skinTex)return;const draw=this.makeBoxDrawable(origin,size,uv,this.skinTex,bone.pivot,inflate,false,false);draw.alpha=.00001;bone.drawables.push(draw);}
  boxRects(u:number,v:number,dx:number,dy:number,dz:number,negateX=false){const eastU0=negateX?u:u+dz+dx,westU0=negateX?u+dz+dx:u;return {top:{r:[u+dz,v,dx,dz],flipU:negateX,flipV:negateX},bottom:{r:[u+dz+dx,v,dx,dz],flipU:negateX,flipV:!negateX},left:{r:[westU0,v+dz,dz,dy],flipU:negateX},front:{r:[u+dz,v+dz,dx,dy],flipU:negateX},right:{r:[eastU0,v+dz,dz,dy],flipU:negateX},back:{r:[u+2*dz+dx,v+dz,dx,dy],flipU:negateX}};}
  makeBoxDrawable(origin:number[],size:number[],uv:number[],tex:any,bonePivot:number[],inflate=0,mirror=false,nrcNegateX=true){const [dx,dy,dz]=size,geom=[dx+inflate*2,dy+inflate*2,dz+inflate*2],center=[origin[0]+dx/2,origin[1]+dy/2,origin[2]+dz/2],local=mt(center[0]-bonePivot[0],center[1]-bonePivot[1],center[2]-bonePivot[2]),rects=this.boxRects(uv[0],uv[1],dx,dy,dz,nrcNegateX),mesh=this.makeBoxMesh(geom,rects,tex,mirror);return {mesh,local,tex,alpha:.00001};}
  faceRect(face:any, fallbackW:number, fallbackH:number){if(!face)return null;const uv=Array.isArray(face)?face:face.uv;if(!Array.isArray(uv)||uv.length<2)return null;const uvSize=!Array.isArray(face)&&Array.isArray(face.uv_size)?face.uv_size:[fallbackW,fallbackH];let u=Number(uv[0])||0,v=Number(uv[1])||0,w=Number(uvSize[0]);let h=Number(uvSize[1]);if(!Number.isFinite(w)||w===0)w=fallbackW;if(!Number.isFinite(h)||h===0)h=fallbackH;let flipU=false,flipV=false;if(w<0){u+=w;w=-w;flipU=true}if(h<0){v+=h;h=-h;flipV=true}return {r:[u,v,w,h],flipU,flipV};}
  geoRects(uv:any,size:number[]){const[dx,dy,dz]=size;if(Array.isArray(uv))return this.boxRects(Number(uv[0])||0,Number(uv[1])||0,dx,dy,dz,false);if(!uv||typeof uv!=="object")return null;return {front:this.faceRect(uv.north,dx,dy),back:this.faceRect(uv.south,dx,dy),left:this.faceRect(uv.west,dz,dy),right:this.faceRect(uv.east,dz,dy),top:this.faceRect(uv.up,dx,dz),bottom:this.faceRect(uv.down,dx,dz)};}
  makeGeoBoxDrawable(cube:any,tex:any,bonePivot:number[],boneMirror=false){const origin=vec3(cube.origin),size=vec3(cube.size,[1,1,1]),inflate=Number(cube.inflate)||0,[dx,dy,dz]=size,geom=[Math.abs(dx)+inflate*2,Math.abs(dy)+inflate*2,Math.abs(dz)+inflate*2],center=[origin[0]+dx/2,origin[1]+dy/2,origin[2]+dz/2],cubePivot=vec3(cube.pivot,center),rotation=vec3(cube.rotation),toPivot=mt(cubePivot[0]-bonePivot[0],cubePivot[1]-bonePivot[1],cubePivot[2]-bonePivot[2]),fromPivot=mt(center[0]-cubePivot[0],center[1]-cubePivot[1],center[2]-cubePivot[2]),local=mm(mm(toPivot,mq(qEulerXYZ(rotation[0],rotation[1],rotation[2]))),fromPivot),rects=this.geoRects(cube.uv,size),mesh=this.makeBoxMesh(geom,rects,tex,Boolean(cube.mirror ?? boneMirror));return {mesh,local,tex,alpha:.00001};}
  makeBoxMesh(size:number[],rects:any,tex:any,mirror=false){const g=this.gl,[x,y,z]=[size[0]/2,size[1]/2,size[2]/2],P:number[]=[],U:number[]=[],N:number[]=[];const face=(name:string,verts:number[][],n:number[])=>{const spec=rects&&rects[name];let r=spec?.r||spec,flipU=!!spec?.flipU,flipV=!!spec?.flipV;if(!r)r=[0,0,tex.w,tex.h];let [u,v,w,h]=r,u0=u/tex.w,u1=(u+w)/tex.w,vt=1-v/tex.h,vb=1-(v+h)/tex.h;if(flipU!==mirror)[u0,u1]=[u1,u0];if(flipV)[vt,vb]=[vb,vt];const uvs=[u0,vt,u1,vt,u1,vb,u0,vb],idx=[0,1,2,0,2,3];for(const i of idx){P.push(...verts[i]);U.push(uvs[i*2],uvs[i*2+1]);N.push(...n)}};face("front",[[-x,y,-z],[x,y,-z],[x,-y,-z],[-x,-y,-z]],[0,0,-1]);face("back",[[x,y,z],[-x,y,z],[-x,-y,z],[x,-y,z]],[0,0,1]);face("left",[[-x,y,z],[-x,y,-z],[-x,-y,-z],[-x,-y,z]],[-1,0,0]);face("right",[[x,y,-z],[x,y,z],[x,-y,z],[x,-y,-z]],[1,0,0]);face("top",[[-x,y,z],[x,y,z],[x,y,-z],[-x,y,-z]],[0,1,0]);face("bottom",[[-x,-y,-z],[x,-y,-z],[x,-y,z],[-x,-y,z]],[0,-1,0]);const buf=(data:number[])=>{const b=g.createBuffer()!;g.bindBuffer(g.ARRAY_BUFFER,b);g.bufferData(g.ARRAY_BUFFER,new Float32Array(data),g.STATIC_DRAW);return b};return {p:buf(P),u:buf(U),n:buf(N),count:P.length/3};}


  makeCapeMesh(tex:any){
    const g=this.gl,P:number[]=[],U:number[]=[],N:number[]=[];
    // Minecraft cape proportions, fitted tightly to the outer skin layer.
    // The upper inside face sits almost directly behind the jacket so the side
    // profile reads as one clean attachment instead of a detached black strip.
    const rows=8,halfW=5,thickness=.44;
    const uv=(u:number,v:number)=>[u/tex.w,1-v/tex.h];
    const pushQuad=(verts:number[][],uvs:number[][],n:number[])=>{const idx=[0,1,2,0,2,3];for(const i of idx){P.push(...verts[i]);U.push(...uvs[i]);N.push(...n)}};
    const point=(side:number,row:number,zOffset:number)=>{const t=row/rows,y=-16*t,z=.12+.14*t*t+zOffset;return [side*halfW,y,z]};
    // Outside (+Z) and inside (-Z) keep the vanilla 64x32 cape layout. The
    // segmented mesh gives a subtle cloth fall without collapsing the side wall.
    for(let row=0;row<rows;row++){
      const t0=row/rows,t1=(row+1)/rows;
      const vo0=1+16*t0,vo1=1+16*t1;
      const outside=[uv(1,vo0),uv(11,vo0),uv(11,vo1),uv(1,vo1)];
      const inside=[uv(12,vo0),uv(22,vo0),uv(22,vo1),uv(12,vo1)];
      const a=point(1,row,thickness/2),b=point(-1,row,thickness/2),c=point(-1,row+1,thickness/2),d=point(1,row+1,thickness/2);
      pushQuad([a,b,c,d],outside,[0,0,1]);
      const ia=point(-1,row,-thickness/2),ib=point(1,row,-thickness/2),ic=point(1,row+1,-thickness/2),id=point(-1,row+1,-thickness/2);
      pushQuad([ia,ib,ic,id],inside,[0,0,-1]);
      // Very thin side seams, sampling the vanilla one-pixel side columns.
      const sideV0=1+16*t0,sideV1=1+16*t1;
      const lu=[uv(0,sideV0),uv(1,sideV0),uv(1,sideV1),uv(0,sideV1)];
      const ru=[uv(11,sideV0),uv(12,sideV0),uv(12,sideV1),uv(11,sideV1)];
      pushQuad([point(-1,row,thickness/2),point(-1,row,-thickness/2),point(-1,row+1,-thickness/2),point(-1,row+1,thickness/2)],lu,[-1,0,0]);
      pushQuad([point(1,row,-thickness/2),point(1,row,thickness/2),point(1,row+1,thickness/2),point(1,row+1,-thickness/2)],ru,[1,0,0]);
    }
    const top=[point(-1,0,thickness/2),point(1,0,thickness/2),point(1,0,-thickness/2),point(-1,0,-thickness/2)];
    pushQuad(top,[uv(1,0),uv(11,0),uv(11,1),uv(1,1)],[0,1,0]);
    const bottom=[point(-1,rows,-thickness/2),point(1,rows,-thickness/2),point(1,rows,thickness/2),point(-1,rows,thickness/2)];
    pushQuad(bottom,[uv(11,0),uv(21,0),uv(21,1),uv(11,1)],[0,-1,0]);
    const buf=(data:number[])=>{const b=g.createBuffer()!;g.bindBuffer(g.ARRAY_BUFFER,b);g.bufferData(g.ARRAY_BUFFER,new Float32Array(data),g.STATIC_DRAW);return b};
    return {p:buf(P),u:buf(U),n:buf(N),count:P.length/3};
  }

  playerAttachment(name:string|undefined,kind:string,boneName="",allowKindFallback=true){const value=(name||boneName||"").toLowerCase().replace(/[^a-z]/g,"");if(value.includes("rightarm")||value.includes("armright"))return this.bones.get("bipedRightArm");if(value.includes("leftarm")||value.includes("armleft"))return this.bones.get("bipedLeftArm");if(value.includes("rightleg")||value.includes("rightboot")||value.includes("legright"))return this.bones.get("bipedRightLeg");if(value.includes("leftleg")||value.includes("leftboot")||value.includes("legleft"))return this.bones.get("bipedLeftLeg");if(value.includes("head")||value.includes("hat"))return this.bones.get("bipedHead");if(value.includes("body")||value.includes("chest")||value.includes("torso"))return this.bones.get("bipedBody");if(value.includes("rig")||value==="root")return this.bones.get("bipedRig");if(!allowKindFallback)return undefined;const k=kind.toLowerCase();if(k==="hat"||k==="halo"||k==="bandana")return this.bones.get("bipedHead");if(k==="cape"||k==="wings"||k==="accessory"||k==="chestplate"||k==="armor")return this.bones.get("bipedBody");return this.bones.get("bipedRig");}
  geometryRoot(model:any){const roots=model?.["minecraft:geometry"] ?? model?.geometry ?? model;if(Array.isArray(roots)){for(const geometry of roots){if(Array.isArray(geometry?.bones))return geometry}}if(Array.isArray(roots?.bones))return roots;return null;}
  bbmodelBones(model:any){
    if(!Array.isArray(model?.groups)||!Array.isArray(model?.elements)||!Array.isArray(model?.outliner))return [];
    const groups=new Map(model.groups.map((group:any)=>[String(group?.uuid||""),group]));
    const elements=new Map(model.elements.map((element:any)=>[String(element?.uuid||""),element]));
    const bones:any[]=[];const byUuid=new Map<string,any>();
    const convertFace=(face:any)=>{const uv=Array.isArray(face?.uv)?face.uv:null;if(!uv||uv.length<4)return undefined;return {uv:[Number(uv[0])||0,Number(uv[1])||0],uv_size:[(Number(uv[2])||0)-(Number(uv[0])||0),(Number(uv[3])||0)-(Number(uv[1])||0)]};};
    // Blockbench's GeckoLib editor stores X/pivot/rotation in its editor coordinate
    // system. The launcher mirrors X to match the Minecraft player rig, but keeps the
    // authored X tilt so the bandana rises at the forehead and falls toward the knot.
    // Y is mirrored with X so the left/right tail rotations stay symmetric.
    const bbPointToBedrock=(value:any)=>{const p=vec3(value);return[-p[0],p[1],p[2]];};
    const bbRotationToBedrock=(value:any)=>{const r=vec3(value);return[r[0],-r[1],r[2]];};
    const cubeFrom=(element:any)=>{if(element?.export===false||!Array.isArray(element?.from)||!Array.isArray(element?.to))return null;const rawFrom=vec3(element.from),rawTo=vec3(element.to),from=[-rawTo[0],rawFrom[1],rawFrom[2]],to=[-rawFrom[0],rawTo[1],rawTo[2]],faces=element.faces||{};const uv:any={};for(const key of ["north","east","south","west","up","down"]){const converted=convertFace(faces[key]);if(converted)uv[key]=converted}const rawPivot=Array.isArray(element?.origin)?element.origin:[(rawFrom[0]+rawTo[0])/2,(rawFrom[1]+rawTo[1])/2,(rawFrom[2]+rawTo[2])/2];return {origin:from,size:[to[0]-from[0],to[1]-from[1],to[2]-from[2]],pivot:bbPointToBedrock(rawPivot),rotation:bbRotationToBedrock(element?.rotation),inflate:Number(element?.inflate)||0,uv};};
    const walk=(node:any,parentName:string|undefined)=>{
      if(typeof node==="string"){const element=elements.get(node);if(!element||!parentName)return;const parent=byUuid.get(parentName);const cube=cubeFrom(element);if(parent&&cube)parent.cubes.push(cube);return;}
      if(!node||typeof node!=="object")return;const uuid=String(node.uuid||"");const group=groups.get(uuid);if(!group)return;const bone:any={name:String(group.name||uuid),pivot:bbPointToBedrock(group.origin),rotation:bbRotationToBedrock(group.rotation),cubes:[]};if(parentName){const parent=byUuid.get(parentName);if(parent)bone.parent=parent.name;}bones.push(bone);byUuid.set(uuid,bone);for(const child of Array.isArray(node.children)?node.children:[])walk(child,uuid);
    };
    for(const root of model.outliner)walk(root,undefined);
    return bones;
  }
  geometryBones(model:any){const bedrock=this.geometryRoot(model)?.bones;if(Array.isArray(bedrock))return bedrock;return this.bbmodelBones(model);}
  geometryTextureSize(model:any){const description=this.geometryRoot(model)?.description;const w=Number(description?.texture_width),h=Number(description?.texture_height);if(Number.isFinite(w)&&w>0&&Number.isFinite(h)&&h>0)return[w,h];const bw=Number(model?.resolution?.width),bh=Number(model?.resolution?.height);return Number.isFinite(bw)&&bw>0&&Number.isFinite(bh)&&bh>0?[bw,bh]:undefined;}
  buildBandanaClientGeometry(source:LoadedRendererCosmetic,tex:any,rawBones:any[]){
    const head=this.bones.get("bipedHead");if(!head)return false;
    const relevantNames=new Set(["bandana","knot","left_tail","left_tail2","right_tail","right_tail2"]);
    const relevant=rawBones.filter((bone:any)=>relevantNames.has(String(bone?.name||"").toLowerCase()));
    if(!relevant.length)return false;
    const byName=new Map<string,any>(relevant.map((bone:any)=>[String(bone?.name||"").toLowerCase(),bone]));
    const created=new Map<string,Bone>();
    const prefix=`cosmetic:${source.asset.id}:bandana-authored:`;
    const create=(name:string):Bone|undefined=>{
      const key=name.toLowerCase();
      if(created.has(key))return created.get(key);
      const sourceBone=byName.get(key);if(!sourceBone)return undefined;
      const parentName=String(sourceBone?.parent||"").toLowerCase();
      const parent=(parentName&&relevantNames.has(parentName)?create(parentName):undefined)??head;
      const pivot=vec3(sourceBone?.pivot,[0,24,0]);
      const node=new Bone(`${prefix}${key}`,pivot,parent,vec3(sourceBone?.rotation));
      // The authored tail meshes are mirrored around the knot, but Blockbench's
      // per-element pivots leave the two strands vertically staggered in this WebGL
      // rig. Keep the model bytes untouched and compensate only at render time so
      // both strands sit next to each other behind the knot, like in Minecraft.
      if(key==="left_tail"){node.pos[1]=.78;node.pos[0]=-.10;}
      else if(key==="left_tail2"){node.pos[0]=-.95;}
      else if(key==="right_tail"){node.pos[1]=-.78;node.pos[0]=.10;}
      else if(key==="right_tail2"){node.pos[0]=.48;}
      this.bones.set(node.name,node);created.set(key,node);
      for(const cube of Array.isArray(sourceBone?.cubes)?sourceBone.cubes:[]){
        if(!cube?.size)continue;
        node.drawables.push(this.makeGeoBoxDrawable(cube,tex,pivot,Boolean(sourceBone?.mirror)));
      }
      return node;
    };
    // Render the exact exported Blockbench/GeckoLib bandana subtree.  Helper player bones
    // (bipedHead/armorHead) are attachment markers only and are intentionally not duplicated.
    for(const name of ["bandana","knot","left_tail","left_tail2","right_tail","right_tail2"])create(name);
    return created.size>0;
  }
  buildCosmeticGeometry(source:LoadedRendererCosmetic,tex:any){let rawBones=this.geometryBones(source.asset.model);if(!rawBones.length)return false;const kind=source.asset.kind.toLowerCase();
    // Bandanas use the exact authored model subtree, attached directly to the player head.
    const def:any=source.asset.definition||{};const bandanaHint=`${source.asset.id} ${source.asset.name} ${kind} ${String(def.category||"")} ${String(def.variantGroup||"")}`.toLowerCase();const isBandana=bandanaHint.includes("bandana")||bandanaHint.includes("tied_bandana");
    if(isBandana&&rawBones.some((bone:any)=>String(bone?.name||"").toLowerCase()==="armorhead"))return this.buildBandanaClientGeometry(source,tex,rawBones);
    const shouldSkipBone=(bone:any)=>{if(!isBandana)return false;const name=String(bone?.name||"").trim().toLowerCase();return name==="head"||name==="preview_head"};
    rawBones=rawBones.filter((bone:any)=>!shouldSkipBone(bone));
    const prefix=`cosmetic:${source.asset.id}:`,created=new Map<string,Bone>(),pending=rawBones.map((bone:any,index:number)=>({bone,index}));let guard=pending.length+3;while(pending.length&&guard-->0){let progressed=false;for(let i=pending.length-1;i>=0;i--){const {bone,index}=pending[i],name=String(bone?.name||`bone_${index}`),parentName=typeof bone?.parent==="string"?bone.parent:"";let parent=parentName?created.get(parentName):undefined;if(parentName&&!parent){parent=this.playerAttachment(parentName,source.asset.kind,parentName,false);if(!parent)continue}if(!parent)parent=this.playerAttachment(undefined,source.asset.kind,name,true) ?? null;const pivot=vec3(bone?.pivot),rotation=vec3(bone?.rotation);const node=new Bone(prefix+name,pivot,parent,rotation);this.bones.set(prefix+name,node);if(!parent)this.topBones.push(node);created.set(name,node);for(const cube of Array.isArray(bone?.cubes)?bone.cubes:[]){if(!cube?.size)continue;node.drawables.push(this.makeGeoBoxDrawable(cube,tex,pivot,Boolean(bone?.mirror)))}pending.splice(i,1);progressed=true}if(!progressed)break}
    // Any malformed orphan bones are still rendered on the rig instead of making the entire cosmetic disappear.
    for(const {bone,index} of pending){if(shouldSkipBone(bone))continue;const name=String(bone?.name||`orphan_${index}`),pivot=vec3(bone?.pivot),node=new Bone(prefix+name,pivot,this.playerAttachment(undefined,source.asset.kind,name)??this.bones.get("bipedRig")!,vec3(bone?.rotation));this.bones.set(prefix+name,node);for(const cube of Array.isArray(bone?.cubes)?bone.cubes:[])if(cube?.size)node.drawables.push(this.makeGeoBoxDrawable(cube,tex,pivot,Boolean(bone?.mirror)));}
    return true;}
  addFallbackCosmetic(source:LoadedRendererCosmetic,tex:any){const kind=source.asset.kind.toLowerCase(),body=this.bones.get("bipedBody")!,head=this.bones.get("bipedHead")!;const full={front:{r:[0,0,tex.w,tex.h]},back:{r:[0,0,tex.w,tex.h]},left:{r:[0,0,tex.w,tex.h]},right:{r:[0,0,tex.w,tex.h]},top:{r:[0,0,tex.w,tex.h]},bottom:{r:[0,0,tex.w,tex.h]}};const drawable=(origin:number[],size:number[],pivot:number[],rotation=[0,0,0])=>{const center=[origin[0]+size[0]/2,origin[1]+size[1]/2,origin[2]+size[2]/2],local=mm(mt(center[0]-pivot[0],center[1]-pivot[1],center[2]-pivot[2]),mq(qEulerXYZ(...rotation))),mesh=this.makeBoxMesh(size,full,tex,false);return{mesh,local,tex,alpha:.00001}};
    if(kind==="cape"){
      // Match the client's PlayerCapeModel more closely: the cape is a thin cloth hinged
      // just behind the jacket layer, not a one-block-thick slab. The mesh itself carries
      // a gentle downward curve, so it stays close to the shoulders and falls naturally.
      const cape=new Bone(`cosmetic:${source.asset.id}:cape`,[0,23.58,2.58],body,[6.5,0,0]);
      cape.drawables.push({mesh:this.makeCapeMesh(tex),local:m4(),tex,alpha:.00001});
      this.bones.set(cape.name,cape);
      this.cosmeticRoots.push({bone:cape,kind,phase:0});
      return true
    }
    if(kind==="wings"){const left=new Bone(`cosmetic:${source.asset.id}:leftWing`,[-3,21,2.5],body,[0,-12,-18]),right=new Bone(`cosmetic:${source.asset.id}:rightWing`,[3,21,2.5],body,[0,12,18]);left.drawables.push(drawable([-10,12,2.5],[7,14,.45],left.pivot));right.drawables.push(drawable([3,12,2.5],[7,14,.45],right.pivot));this.cosmeticRoots.push({bone:left,kind,phase:0},{bone:right,kind,phase:Math.PI});return true}
    if(kind==="halo"){const halo=new Bone(`cosmetic:${source.asset.id}:halo`,[0,33,0],head);halo.drawables.push(drawable([-5,32.5,-.4],[10,1,.8],halo.pivot),drawable([-.5,32.5,-5],[1,1,10],halo.pivot));this.cosmeticRoots.push({bone:halo,kind,phase:0});return true}
    return false;}
  buildCosmetics(){for(const source of this.cosmeticSources){try{const kind=source.asset.kind.toLowerCase();if(kind==="glint"||source.asset.id.toLowerCase().includes("glint"))continue;if(!source.image)continue;const declared=this.geometryTextureSize(source.asset.model);const logical=declared ?? (kind==="cape" ? [64,32] : undefined);const tex=this.textureFromImage(source.image,true,logical);if(source.asset.model&&this.buildCosmeticGeometry(source,tex))continue;this.addFallbackCosmetic(source,tex)}catch(error){console.warn("[SNine Launcher] Cosmetic render skipped",source.asset.id,error)}}}

  resize(){const d=Math.min(devicePixelRatio||1,2),w=Math.max(1,Math.floor(this.canvas.clientWidth*d)),h=Math.max(1,Math.floor(this.canvas.clientHeight*d));if(this.canvas.width!==w||this.canvas.height!==h){this.canvas.width=w;this.canvas.height=h}this.gl.viewport(0,0,w,h);}
  camera(){const target=this.target,y=this.cameraYaw*D2R,p=this.cameraPitch*D2R,d=this.cameraDistance,eye=[target[0]+Math.sin(y)*Math.cos(p)*d,target[1]+Math.sin(p)*d,target[2]-Math.cos(y)*Math.cos(p)*d],V=lookAt(eye,target,[0,1,0]),P=perspective(32*D2R,this.canvas.width/this.canvas.height,.1,300);return {VP:mm(P,V)};}
  drawBone(b:Bone,VP:Float32Array){const g=this.gl,pr=this.program;for(const d of b.drawables){const M=mm(b.world,d.local),MVP=mm(VP,M);g.useProgram(pr.p);g.bindBuffer(g.ARRAY_BUFFER,d.mesh.p);g.enableVertexAttribArray(pr.aP);g.vertexAttribPointer(pr.aP,3,g.FLOAT,false,0,0);g.bindBuffer(g.ARRAY_BUFFER,d.mesh.u);g.enableVertexAttribArray(pr.aU);g.vertexAttribPointer(pr.aU,2,g.FLOAT,false,0,0);g.bindBuffer(g.ARRAY_BUFFER,d.mesh.n);g.enableVertexAttribArray(pr.aN);g.vertexAttribPointer(pr.aN,3,g.FLOAT,false,0,0);g.uniformMatrix4fv(pr.uM,false,M);g.uniformMatrix4fv(pr.uMVP,false,MVP);g.uniform1f(pr.uAlpha,d.alpha);g.uniform1f(pr.uTime,this.renderTime);g.uniform1f(pr.uGlintStyle,this.glintStyle);g.activeTexture(g.TEXTURE0);g.bindTexture(g.TEXTURE_2D,d.tex.gl);g.uniform1i(pr.uT,0);g.uniform1f(pr.uGlintPass,0);g.drawArrays(g.TRIANGLES,0,d.mesh.count)}for(const c of b.children)this.drawBone(c,VP);}
  frame(now:number){if(!this.running)return;this.renderTime=now/1000;this.resize();const rig=this.bones.get("bipedRig"),head=this.bones.get("bipedHead"),body=this.bones.get("bipedBody"),ra=this.bones.get("bipedRightArm"),la=this.bones.get("bipedLeftArm"),rl=this.bones.get("bipedRightLeg"),ll=this.bones.get("bipedLeftLeg");if(rig&&head&&body&&ra&&la&&rl&&ll){rig.pos[0]=0;rig.pos[1]=0;rig.pos[2]=0;rig.rot[0]=0;rig.rot[1]=0;rig.rot[2]=0;head.rot[0]=0;head.rot[1]=0;head.rot[2]=0;body.rot[0]=0;body.rot[1]=0;body.rot[2]=0;ra.rot[0]=0;ra.rot[1]=0;ra.rot[2]=0;la.rot[0]=0;la.rot[1]=0;la.rot[2]=0;rl.rot[0]=0;rl.rot[1]=0;rl.rot[2]=0;ll.rot[0]=0;ll.rot[1]=0;ll.rot[2]=0;for(const item of this.cosmeticRoots){item.bone.rot[0]=0;item.bone.rot[1]=0;item.bone.rot[2]=0;}}for(const b of this.topBones)b.update(m4());const g=this.gl,{VP}=this.camera();g.enable(g.DEPTH_TEST);g.depthFunc(g.LEQUAL);g.disable(g.CULL_FACE);g.enable(g.BLEND);g.blendFunc(g.SRC_ALPHA,g.ONE_MINUS_SRC_ALPHA);g.clearColor(0,0,0,0);g.clear(g.COLOR_BUFFER_BIT|g.DEPTH_BUFFER_BIT);for(const b of this.topBones)this.drawBone(b,VP);this.frameHandle=requestAnimationFrame(this.frame);}
  dispose(){this.running=false;cancelAnimationFrame(this.frameHandle);for(const t of this.textures)this.gl.deleteTexture(t);this.textures.clear();this.cosmeticTextures.clear();this.gl.deleteProgram(this.program.p);}
}

export function loadSkinImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    if (/^https?:\/\//i.test(url)) image.crossOrigin = "anonymous";
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(`Unable to load image: ${url}`));
    image.src = url;
  });
}
