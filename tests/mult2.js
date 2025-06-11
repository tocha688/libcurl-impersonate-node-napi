const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo, CurlMulti2 } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

console.log("lib路径", getLibPath())

globalInit(3)

console.log("Creating curl instances...")
const curl = new Curl()
curl.init() // 先初始化
curl.setOptString(CurlOpt.Url, "https://httpbin.org/get")
curl.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl.setOptLong(CurlOpt.SslVerifyHost, 0)
curl.impersonate("chrome136", true)

console.log("Curl 1 is valid:", curl.isValid())


console.log("Creating Multi instance...")
const multi = new CurlMulti2()

let completed = 0
const total = 2

function checkComplete() {
    if (completed >= total) {
        console.log("All requests completed!")
        process.exit(0)
    }
}
const CURL_SOCKET_TIMEOUT = -1;
multi.setSocketCallback((err, data) => {
    console.log(`Socket Callback`, data)
})
multi.setTimerCallback((err, data) => {
    console.log(`Timer Callback`, data)
})

console.log("Adding curl handle...")
multi.addHandle(curl)

console.log("Starting initial perform...")
let remaining = multi.perform()
console.log(`Initial perform result: ${remaining} transfers remaining`)

console.log("Requests started, waiting for completion...")

// 持续调用 perform 来驱动请求完成
const performInterval = setInterval(() => {
    try {
        const prevRemaining = remaining
        remaining = multi.perform()
        console.log(`Perform: ${remaining} transfers remaining (was ${prevRemaining})`)

         //获取信息
         const info = multi.infoRead()
         console.log("Info Read:", info)
    } catch (error) {
        console.error("Error in perform:", error)
        clearInterval(performInterval)
    }
}, 100) // 每100ms调用一次

// 30秒超时
// setTimeout(() => {
//     console.log(curl.getRespBody())
//     console.log("Timeout reached")
//     process.exit(0)
// }, 30000)



