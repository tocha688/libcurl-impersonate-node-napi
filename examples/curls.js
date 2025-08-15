const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

const curl1 = new Curl();
const curl2 = new Curl();

// 设置两个不同的请求
curl1.setOption(CurlOpt.Url, "https://tls.peet.ws/api/all");
curl2.setOption(CurlOpt.Url, "https://tls.peet.ws/api/all");

curl1.setOption(CurlOpt.SslVerifyPeer, 0);
curl1.setOption(CurlOpt.SslVerifyHost, 0);

curl2.setOption(CurlOpt.SslVerifyPeer, 0);
curl2.setOption(CurlOpt.SslVerifyHost, 0);

// 并发执行
console.time('concurrent');
Promise.all([
  curl1.perform(),
  curl2.perform()
]).then(() => {
  console.timeEnd('concurrent'); // 应该是 ~3秒，不是 5秒
});

// 同时设置一个定时器
setInterval(() => {
  console.log('Timer still running:', new Date().toISOString());
}, 500);
