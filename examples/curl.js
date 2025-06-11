const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

console.log("lib路径", getLibPath())

globalInit(3)

const curl = new Curl()
// curl.setOptString(CurlOpt.Url, "https://tls.peet.ws/api/all")
curl.setOptString(CurlOpt.Url, "https://www.google.com")
curl.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl.setOptLong(CurlOpt.SslVerifyHost, 0)
//重定向
curl.setOptBool(CurlOpt.FollowLocation, true)
curl.setOptLong(CurlOpt.MaxRedirs, 10)
//自动解码
curl.setOptString(CurlOpt.AcceptEncoding, "")

curl.setCookies("testcookie=1234567890; testcookie2=999999")

curl.impersonate("chrome136", true)
curl.addHeader("h1","w1")
curl.addHeader("h2","w1")
// curl.setHeaders([
//   "h1: v1",
//   "h2: v2",
//   "h3: v3",
//   "h4: v4",
//   "h5: v5",
// ])

// curl.init()

console.log("Starting request...")

// 执行请求 - 同步等待完成
let res = curl.perform()

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

console.log("Curl instance closed..")