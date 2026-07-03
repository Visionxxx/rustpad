// Rustpad – terminal-notepad for txt/md/ini m.m. med md-visning (^P)
use crossterm::{cursor::MoveTo,event::{read,Event,KeyCode::*,KeyEventKind,KeyModifiers as M},execute,queue,
style::{Color::*,Print,ResetColor,SetBackgroundColor as Bg,SetForegroundColor as Fg},terminal::*};
use std::io::{stdout,Write};
fn b(s:&str,i:usize)->usize{s.char_indices().nth(i).map(|(b,_)|b).unwrap_or(s.len())}
fn main()->std::io::Result<()>{
let p=std::env::args().nth(1).unwrap_or("uten_navn.txt".into());
let mut l:Vec<String>=std::fs::read_to_string(&p).unwrap_or_default().lines().map(|s|s.into()).collect();
if l.is_empty(){l.push(String::new())}
let ini=[".ini",".toml",".conf",".cfg"].iter().any(|e|p.ends_with(e));
let(mut cx,mut cy,mut off,mut pv,mut dy)=(0usize,0usize,0usize,false,false);
let mut msg=String::new();
enable_raw_mode()?;execute!(stdout(),EnterAlternateScreen)?;
loop{
let(w,h)=size()?;let(w,h)=(w.max(20)as usize,(h.max(3)as usize)-1);
if cy<off{off=cy}if cy>=off+h{off=cy-h+1}
let mut o=stdout();queue!(o,Clear(ClearType::All))?;
let mut cb=false;
for i in 0..h{
queue!(o,MoveTo(0,i as u16))?;
match l.get(off+i){
None=>{queue!(o,Fg(DarkGrey),Print("~"))?}
Some(s)=>{
let s:String=s.chars().take(w).collect();let t=s.trim_start();
if pv{
if t.starts_with("```"){cb=!cb;queue!(o,Fg(DarkGrey),Print(&s))?}
else if cb{queue!(o,Fg(Green),Print(&s))?}
else if s.starts_with('#'){queue!(o,Fg(Cyan),Print(&s))?}
else if t.starts_with("- ")||t.starts_with("* "){queue!(o,Fg(Magenta),Print(" • "),ResetColor,Print(&t[2..]))?}
else if t.starts_with('>'){queue!(o,Fg(Yellow),Print(&s))?}
else{queue!(o,Print(&s))?}
}else if ini&&(t.starts_with('[')||t.starts_with(';')||t.starts_with('#')){
queue!(o,Fg(if t.starts_with('['){Cyan}else{DarkGrey}),Print(&s))?
}else{queue!(o,Print(&s))?}}}
queue!(o,ResetColor)?}
let st=format!(" RUSTPAD  {}{}  {}:{} {}  ^S=lagre ^P=md ^Q=slutt {}",
p,if dy{"*"}else{""},cy+1,cx+1,if pv{"[VIS]"}else{"[RED]"},msg);
let st:String=st.chars().take(w).collect();
queue!(o,MoveTo(0,h as u16),Bg(White),Fg(Black),Print(format!("{:<1$}",st,w)),ResetColor,
MoveTo(cx as u16,(cy-off)as u16))?;o.flush()?;
if let Event::Key(k)=read()?{
if k.kind!=KeyEventKind::Press{continue}
msg.clear();
let c=k.modifiers.contains(M::CONTROL);
match k.code{
Char('q')if c=>break,
Char('s')if c=>{std::fs::write(&p,l.join("\n")+"\n")?;dy=false;msg="Lagret!".into()}
Char('p')if c=>pv=!pv,
Up=>cy=cy.saturating_sub(1),
Down=>if cy+1<l.len(){cy+=1},
PageUp=>cy=cy.saturating_sub(h),
PageDown=>cy=(cy+h).min(l.len()-1),
Home=>cx=0,
End=>cx=l[cy].chars().count(),
Left=>if cx>0{cx-=1}else if cy>0{cy-=1;cx=l[cy].chars().count()},
Right=>if cx<l[cy].chars().count(){cx+=1}else if cy+1<l.len(){cy+=1;cx=0},
Enter=>{let i=b(&l[cy],cx);let r=l[cy].split_off(i);l.insert(cy+1,r);cy+=1;cx=0;dy=true}
Backspace=>if cx>0{let i=b(&l[cy],cx-1);l[cy].remove(i);cx-=1;dy=true}
else if cy>0{let s=l.remove(cy);cy-=1;cx=l[cy].chars().count();l[cy].push_str(&s);dy=true},
Delete=>if cx<l[cy].chars().count(){let i=b(&l[cy],cx);l[cy].remove(i);dy=true}
else if cy+1<l.len(){let s=l.remove(cy+1);l[cy].push_str(&s);dy=true},
Tab=>{let i=b(&l[cy],cx);l[cy].insert_str(i,"  ");cx+=2;dy=true}
Char(ch)if !c=>{let i=b(&l[cy],cx);l[cy].insert(i,ch);cx+=1;dy=true}
_=>{}}
cx=cx.min(l[cy].chars().count())}}
execute!(stdout(),LeaveAlternateScreen)?;disable_raw_mode()}
