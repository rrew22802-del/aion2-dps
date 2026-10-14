// NPC spawn description/state prefix. Only structurally recognized HP pairs are accepted.
pub(super) fn spawn_vitals(bytes: &[u8]) -> Option<(u32,u32,u32,u32)> {
    if !bytes.starts_with(&[0x41,0x36]) { return None; }
    for flag_width in [2usize,4] {
        if let Some(value)=read_prefix(bytes,flag_width) { return Some(value); }
    }
    None
}
fn read_prefix(bytes:&[u8],flag_width:usize)->Option<(u32,u32,u32,u32)> {
    let mut at=2;
    let entity=var(bytes,&mut at)?;
    if entity==0 || entity>u32::MAX as u64 {return None;}
    let flags=take(bytes,&mut at,flag_width)?;
    if flag_width==4 && u32::from_le_bytes(flags.try_into().ok()?) & !0x1ffff != 0 { return None; }
    let desc=take(bytes,&mut at,1)?[0];
    // Custom string descriptions have a separate wire layout; leave them unknown.
    if desc & !0x1e !=0 {return None;}
    let code=u32::from_le_bytes(take(bytes,&mut at,4)?.try_into().ok()?);
    if !(2_000_000..3_000_000).contains(&code) {return None;}
    for (bit,count) in [(2,1),(4,1),(8,5),(16,1)] { if desc & bit !=0 {take(bytes,&mut at,count)?;} }
    let state=u16::from_le_bytes(take(bytes,&mut at,2)?.try_into().ok()?);
    take(bytes,&mut at,18)?;
    for (bit,count) in [(1,12),(2,4),(4,2),(8,12),(16,12),(32,4)] { if state & bit !=0 {take(bytes,&mut at,count)?;} }
    take(bytes,&mut at,1)?;
    let current=var(bytes,&mut at)?; let maximum=var(bytes,&mut at)?;
    take(bytes,&mut at,52)?;
    if maximum==0 || maximum>u32::MAX as u64 || current>maximum {return None;}
    Some((entity as u32,code,current as u32,maximum as u32))
}
fn take<'a>(bytes:&'a [u8],at:&mut usize,n:usize)->Option<&'a [u8]> {
    let end=at.checked_add(n)?; let value=bytes.get(*at..end)?; *at=end; Some(value)
}
fn var(bytes:&[u8],at:&mut usize)->Option<u64> {
    let mut value=0u64;
    for shift in (0..64).step_by(7) {
        let byte=take(bytes,at,1)?[0];
        if shift==63 && byte>1 {return None;}
        value|=((byte & 127) as u64)<<shift;
        if byte & 128==0 {return Some(value);}
    }
    None
}
#[cfg(test)] mod tests {
    use super::*;
    fn encode(mut n:u64)->Vec<u8>{let mut b=Vec::new();loop{let mut x=(n &127) as u8;n>>=7;if n>0{x|=128;}b.push(x);if n==0{return b;}}}
    #[test] fn hp_is_a_pair_not_an_observed_peak(){
        let mut b=vec![0x41,0x36]; b.extend(encode(9001));b.extend([0,0,0]);b.extend(2_400_017u32.to_le_bytes());b.extend([0;21]);
        b.extend(encode(1_889_096));b.extend(encode(15_504_000));b.extend([0;52]);
        assert_eq!(spawn_vitals(&b),Some((9001,2_400_017,1_889_096,15_504_000)));
        for end in 0..b.len(){assert_eq!(spawn_vitals(&b[..end]),None);}
        b[0]=0;assert_eq!(spawn_vitals(&b),None);
    }
}

#[cfg(test)] mod offline_replay {
    use super::*;
    #[test] fn inspect_aligned_spawn_capture() {
        let Ok(path)=std::env::var("DBAION2_NPC_REPLAY") else{return;};
        let text=std::fs::read_to_string(path).unwrap(); let mut total=0;let mut decoded=0;
        for line in text.lines(){let b:Vec<u8>=line.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(),16).unwrap()).collect();total+=1;if let Some((_,_,current,max))=spawn_vitals(&b){decoded+=1;println!("Confirmed replay pair: current={current}, max={max}");}}
        println!("Aligned replay spawn frames={total}, recognized layouts={decoded}");
    }
}
