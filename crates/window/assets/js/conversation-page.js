(function(){
var opened=new Set(),following=true,top=0,conversation=null;
function bottom(){scrollTo(0,document.documentElement.scrollHeight);}
// When each message was said, in the reader's own time: the hour alone for
// today's, the day too for an earlier one's.
function times(root){
  root.querySelectorAll('time[data-at]').forEach(function(t){
    if(t.textContent)return;
    var at=new Date(Number(t.getAttribute('data-at')));
    if(isNaN(at))return;
    var today=new Date().toDateString()===at.toDateString();
    t.textContent=today?at.toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})
      :at.toLocaleString([],{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
    t.setAttribute('title',at.toLocaleString());
  });
}
addEventListener('resize',function(){if(following)bottom();});
addEventListener('scroll',function(){
  top=scrollY;
  following=document.documentElement.scrollHeight-scrollY-innerHeight<64;
},{passive:true});
// Run before the Markdown page replaces the contents, while the old scroll
// height still tells whether the user was following the latest reply.
addEventListener('message',function(e){
  var m=e.data;
  if(e.source!==parent || !m || m.tsumugiViewer!==1 || typeof m.markdown!=='string')return;
  top=scrollY;
  if(typeof m.conversation==='string' && m.conversation!==conversation){
    conversation=m.conversation;opened.clear();following=true;top=0;
  }
});
document.addEventListener('toggle',function(e){
  var d=e.target;if(!d.matches||!d.matches('details[data-progress]'))return;
  var id=d.getAttribute('data-progress');
  if(d.open)opened.add(id);else opened.delete(id);
},true);
document.addEventListener('click',function(e){
  var b=e.target&&e.target.closest&&e.target.closest('[data-conversation-copy]');
  if(b)parent.postMessage({tsumugiConversationCopy:1,text:b.getAttribute('data-conversation-copy')},'*');
});
document.addEventListener('mousedown',function(){
  parent.postMessage({tsumugiConversationFocus:1},'*');
});
document.addEventListener('DOMContentLoaded',function(){
  var thread=document.querySelector('.conversation-thread');
  if(!thread)return;
  conversation=thread.getAttribute('data-conversation');
  times(thread);
  new MutationObserver(function(){
    times(thread);
    thread.querySelectorAll('details[data-progress]').forEach(function(d){
      d.open=opened.has(d.getAttribute('data-progress'));
    });
    if(following)requestAnimationFrame(bottom);else scrollTo(0,top);
  }).observe(thread,{childList:true});
  // What is drawn after the replies are put in, as highlighted code or an
  // image read, makes the page taller with no change to its children.
  if(typeof ResizeObserver==='function')new ResizeObserver(function(){
    if(following)bottom();
  }).observe(thread);
  requestAnimationFrame(bottom);
});
})();
