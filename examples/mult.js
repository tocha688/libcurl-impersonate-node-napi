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


console.log("Creating Multi instance...")
const multi = new CurlMulti()

multi.setSocketCallback((data) => {
    console.log("Socket Callback:", data)
})
multi.setTimerCallback((data) => {
    console.log("Timer Callback:", data)
})

multi.addHandle(curl)
let remaining = multi.perform()
console.time("ping")
setInterval(() => {
    console.timeLog("ping")
}, 50)
while (remaining > 0) {
    const prevRemaining = remaining;
    console.log("返回", await multi.poll(10000))
    remaining = multi.perform()
    console.log(`Perform: ${remaining} transfers remaining (was ${prevRemaining})`)
    // 检查是否有传输完成
    if (prevRemaining > remaining || remaining === 0) {
        console.log("检查完成的传输:", remaining)

        // 持续读取所有可用的消息
        let hasMessages = true
        while (hasMessages) {
            const info = multi.infoRead()
            if (info) {
                console.log("Info Read:", info)
                // 处理完成的传输
                if (info.msg === 1) { // CURLMSG_DONE
                    console.log("传输完成，结果码:", info.data)
                    // 可以在这里获取响应数据
                    console.log("响应体:", curl.getRespBody())
                }
            } else {
                hasMessages = false
            }
        }
    }
}


