const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo, CurlMulti } = require("..")
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

const curl2 = new Curl()
curl2.init() // 先初始化
curl2.setOptString(CurlOpt.Url, "https://httpbin.org/get")
curl2.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl2.setOptLong(CurlOpt.SslVerifyHost, 0)
curl2.impersonate("chrome136", true)

console.log("Curl 2 is valid:", curl2.isValid())

console.log("Creating Multi instance...")
const multi = new CurlMulti()

let completed = 0
const total = 2

function checkComplete() {
    if (completed >= total) {
        console.log("All requests completed!")
        process.exit(0)
    }
}

console.log("Starting requests...")
multi.send(curl, (result) => {
    console.log("Request 1 completed!", result)
    completed++
    checkComplete()
    console.log("响应内容", Buffer.from(curl.getRespBody()).toString("utf8"))
}, (error) => {
    console.log("Request 1 error!", error)
    completed++
    checkComplete()
})

multi.send(curl2, (result) => {
    console.log("Request 2 completed!", result)
    completed++
    checkComplete()
    console.log("响应内容", Buffer.from(curl2.getRespBody()).toString("utf8"))
}, (error) => {
    console.log("Request 2 error!", error)
    completed++
    checkComplete()
})

console.log("Requests started, waiting for completion...")

// // 持续调用 perform 来驱动请求完成
// const performInterval = setInterval(() => {
//     try {
//         const remaining = multi.perform()
//         console.log(`Remaining transfers: ${remaining}`)

//         if (remaining === 0) {
//             console.log("No more transfers, stopping interval")
//             clearInterval(performInterval)
//         }
//     } catch (error) {
//         console.error("Error in perform:", error)
//         clearInterval(performInterval)
//     }
// }, 100) // 每100ms调用一次

// 30秒超时
setTimeout(() => {
    console.log("Timeout reached.")
    clearInterval(performInterval)
    process.exit(0)
}, 30000)



