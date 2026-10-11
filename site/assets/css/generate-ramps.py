import math, json
exec(open('palette.py').read().split('# Target lightness')[0].replace('print(','pass#('))

ORDER=[50,100,200,300,400,500,600,700,800,900,950]
BASE ={50:0.985,100:0.955,200:0.912,300:0.855,400:0.782,
       500:0.694,600:0.600,700:0.502,800:0.406,900:0.320,950:0.238}
GROUND=hex2rgb(P["paper"])

def lum_(rgb):
    r,g,b=[s2l(c) for c in rgb]; return 0.2126*r+0.7152*g+0.0722*b
def contrast(a,b):
    la,lb=lum_(a),lum_(b); hi,lo=max(la,lb),min(la,lb); return (hi+0.05)/(lo+0.05)

ramps={}; report=[]
for name,hexv in P.items():
    L0,C0,h0=lch(hex2rgb(hexv))
    pin=min(ORDER,key=lambda s:abs(BASE[s]-L0))
    # shift every step by how far the swatch sits from its nominal slot,
    # tapering to zero at both ends so 50 stays near-white and 950 stays dark
    d=L0-BASE[pin]
    i_pin=ORDER.index(pin)
    Ls={}
    for i,s in enumerate(ORDER):
        taper = 1-abs(i-i_pin)/max(i_pin, len(ORDER)-1-i_pin, 1)
        Ls[s]=min(0.99, max(0.20, BASE[s]+d*max(0.0,taper)))
    # enforce strictly descending lightness
    for i in range(1,len(ORDER)):
        a,b=ORDER[i-1],ORDER[i]
        if Ls[b]>=Ls[a]-0.012: Ls[b]=Ls[a]-0.012
    ramp={}
    for s in ORDER:
        if s==pin: rgb=hex2rgb(hexv); C=C0
        else:
            k=1.0 if Ls[s]>=L0 else min(1.5,1.0+(L0-Ls[s])*1.3)
            C=C0*k*(0.5 if Ls[s]>0.96 else 1.0)
            rgb=from_lch(Ls[s],C,h0)
        ramp[s]=rgb2hex(rgb)
    ramps[name]={'pin':pin,'steps':ramp}
    mono=all(lch(hex2rgb(ramp[ORDER[i]]))[0] > lch(hex2rgb(ramp[ORDER[i+1]]))[0] for i in range(len(ORDER)-1))
    text=[s for s in ORDER if contrast(hex2rgb(ramp[s]),GROUND)>=4.5]
    large=[s for s in ORDER if contrast(hex2rgb(ramp[s]),GROUND)>=3.0]
    report.append((name,pin,mono,min(text) if text else None,min(large) if large else None,ramp))

print(f"{'hue':8} {'pin':>4} {'mono':>5} {'AA@':>5} {'AAlg@':>6}  ramp")
for name,pin,mono,aa,lg,ramp in report:
    print(f"{name:8} {pin:>4} {'ok' if mono else 'BAD':>5} {str(aa):>5} {str(lg):>6}  " + ' '.join(ramp[s] for s in ORDER))
json.dump({k:v['steps'] for k,v in ramps.items()}, open('ramps.json','w'), indent=1)
print('\nink on paper:', round(contrast(hex2rgb(ramps['sky']['steps'][950]), GROUND),2),
      '| body candidate sky-900:', round(contrast(hex2rgb(ramps['sky']['steps'][900]), GROUND),2))
