const dbg = x => {
  console.log(x);
  return x;
};
const client = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/connect`);
const iframe = dbg(document.querySelector("iframe"));

client.onmessage = ({ data }) => {
  const slide = data;
  iframe.src = `/page/${slide}`;
};

(() => document.addEventListener("keydown", ({ key }) => {
  switch (key) {
    case "ArrowLeft": case "ArrowUp": {
      iframe.src = `/page/${Math.max(0, parseInt(iframe.contentWindow?.location.pathname.slice(6) ?? "0") - 1)}`;
    } break;
    case "ArrowRight": case "ArrowDown": {
      iframe.src = `/page/${Math.min(LENGTH - 1, parseInt(iframe.contentWindow?.location.pathname.slice(6) ?? "0") + 1)}`;
    } break;
  }
}))
