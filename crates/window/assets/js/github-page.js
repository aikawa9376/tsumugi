(function(){
// The pane shown: Conversation, Files changed or Checks. It stays chosen when
// what is read again replaces the page's contents.
var shown='conversation';
// The card the keys act on, by its number; none until one is selected.
var selected=-1;
function mark(scroll){
  Array.prototype.forEach.call(document.querySelectorAll('.gh-post[data-card]'),function(c){
    var on=Number(c.getAttribute('data-card'))===selected;
    c.classList.toggle('gh-selected',on);
    if(on&&scroll&&c.scrollIntoView)c.scrollIntoView({block:'nearest'});
  });
}
window.addEventListener('message',function(e){
  var m=e.data;
  if(m&&typeof m.select==='number'){selected=m.select;mark(true);}
});
function show(){
  Array.prototype.forEach.call(document.querySelectorAll('.gh-tab[data-pane]'),function(t){
    t.classList.toggle('on',t.getAttribute('data-pane')===shown);
  });
  Array.prototype.forEach.call(document.querySelectorAll('.gh-pane[data-pane]'),function(p){
    p.hidden=p.getAttribute('data-pane')!==shown;
  });
  mark(false);
}
document.addEventListener('click',function(e){
  var tab=e.target&&e.target.closest&&e.target.closest('.gh-tab[data-pane]');
  if(tab){shown=tab.getAttribute('data-pane');show();scrollTo(0,0);return;}
  // A card pressed is the one the keys act on; the window keeps the number.
  var card=e.target&&e.target.closest&&e.target.closest('.gh-post[data-card]');
  if(card&&!(e.target.closest('a[href]')||e.target.closest('input'))){
    parent.postMessage({tsumugiGithub:'select',card:Number(card.getAttribute('data-card'))},'*');
  }
  // A file's box marks it viewed or not, on GitHub, by the window.
  var box=e.target&&e.target.closest&&e.target.closest('input[data-viewed]');
  if(box){
    parent.postMessage({tsumugiGithub:'viewed',path:box.getAttribute('data-viewed'),viewed:box.checked},'*');
    return;
  }
  // The frame navigates nowhere: a link to an address is told to the window by
  // the document's script, which opens it, and the document's own places are
  // scrolled to by that script.
  var a=e.target&&e.target.closest&&e.target.closest('a[href]');
  if(!a)return;
  var href=a.getAttribute('href')||'';
  if(href.charAt(0)==='#')return;
  e.preventDefault();
  e.stopPropagation();
},true);
document.addEventListener('DOMContentLoaded',function(){
  var thread=document.querySelector('.gh-thread');
  if(thread)new MutationObserver(show).observe(thread,{childList:true});
});
})();
