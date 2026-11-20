(function(){
var f=document.getElementById(id);
if(!f)return;
f.tsumugiConversationMessage=Object.assign({tsumugiViewer:1},message);
function send(){if(f.contentWindow)f.contentWindow.postMessage(f.tsumugiConversationMessage,'*');}
// An update sent while srcdoc is still loading has no receiver. Keep the
// latest snapshot on this frame so even an idle session recovers on load.
if(!f.tsumugiConversationReplay){
  f.tsumugiConversationReplay=true;
  f.addEventListener('load',send);
}
send();
})();
