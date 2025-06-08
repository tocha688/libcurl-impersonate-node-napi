const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

console.log("lib路径", getLibPath())

globalInit(3)

const curl = new Curl()
curl.setOptString(CurlOpt.Url, "https://tls.peet.ws/api/all")
curl.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl.setOptLong(CurlOpt.SslVerifyHost, 0)

console.log("Starting request...")

// 执行请求 - 同步等待完成
let res = curl.perform()

console.log("Request completed!")
console.log("Response:", res)
console.log("Response Code:", curl.getInfoNumber(CurlInfo.ResponseCode))

// 直接获取完整的数据
console.log("\n=== Headers ===")
console.log(Buffer.from(curl.getHeaders()).toString("utf8"))

console.log("\n=== Body ===")
console.log(Buffer.from(curl.getBody()).toString("utf8"))

curl.close()