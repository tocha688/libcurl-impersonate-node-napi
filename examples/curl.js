const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

console.log("lib路径", getLibPath())

globalInit(3)

const curl = new Curl()
// curl.setOption(CurlOpt.Url, "https://tls.peet.ws/api/all")
curl.setOption(CurlOpt.Url, "https://www.google.com")
curl.setOption(CurlOpt.SslVerifyPeer, 0)
curl.setOption(CurlOpt.SslVerifyHost, 0)
//重定向
curl.setOption(CurlOpt.FollowLocation, true)
curl.setOption(CurlOpt.MaxRedirs, 10)
//自动解码
curl.setOption(CurlOpt.AcceptEncoding, "")

curl.setCookies("testcookie=1234567890; testcookie2=999999")

curl.impersonate("chrome136", true)


console.log("Starting request...")

let timer = setInterval(() => {
    console.log("Request is still running...")
}, 100)
// 执行请求 - 同步等待完成
let res = await curl.perform()

console.log("Request completed!")
console.log("Response:", res)
console.log("Response Code:", curl.getInfoNumber(CurlInfo.ResponseCode))

// 直接获取完整的数据
console.log("\n=== Headers ===")
console.log(Buffer.from(curl.getRespHeaders()).toString("utf8"))

console.log("\n=== Cookie ===")
curl.getCookies().forEach(cookie => {
    console.log(cookie)
})

// console.log("\n=== Body ===")
// console.log(Buffer.from(curl.getRespBody()).toString("utf8"))

// curl.reset();

curl.close()
clearInterval(timer);
console.log("Curl instance closed.")
