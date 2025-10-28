const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo, CurlMulti } = require("..")
const path = require("path")

async function main() {
  setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))
  console.log("lib路径", getLibPath())

  globalInit(3)

  console.log("Creating curl instance...")
  const curl = new Curl()
  curl.init() // 初始化数据回调
  curl.setOption(CurlOpt.Url, "https://httpbin.org/get")
  curl.setOption(CurlOpt.SslVerifyPeer, 0)
  curl.setOption(CurlOpt.SslVerifyHost, 0)
  curl.impersonate("chrome136", true)

  console.log("Creating Multi instance...")
  const multi = new CurlMulti()

  // 可选：观察 socket/timer 事件
  multi.setSocketCallback((data) => {
    console.log("Socket:", data)
  })
  multi.setTimerCallback((data) => {
    console.log("Timer:", data)
  })

  let timer
  try {
    // 添加 handle 并开始循环
    multi.addHandle(curl)

    console.time("ping")
    timer = setInterval(() => {
      console.timeLog("ping")
    }, 200)

    let remaining = multi.perform()
    while (remaining > 0) {
      const prevRemaining = remaining

      // 使用 AsyncTask 风格的 wait/poll，不返回数值，因此无需打印
      await multi.wait(100) // 或者使用 await multi.poll(100)

      remaining = multi.perform()
      console.log(`Perform: ${remaining} transfers remaining (was ${prevRemaining})`)

      // 读取完成消息
      if (prevRemaining > remaining || remaining === 0) {
        let info
        do {
          info = multi.infoRead()
          if (info) {
            console.log("Info:", info)
            if (info.msg === 1) { // CURLMSG_DONE
              console.log("传输完成，结果码:", info.data)
              console.log("状态码:", curl.status())
              console.log("响应体长度:", curl.getRespBody().toString("utf-8"))
              return;
            }
          }
        } while (info)
      }
    }

  } finally {
    console.timeEnd("ping")
    if (timer) clearInterval(timer)
    // 尽量清理资源
    try { multi.removeHandle(curl) } catch {}
    try { multi.close() } catch {}
    try { curl.close() } catch {}
  }
}

// CommonJS 中不支持顶层 await，使用 main 包裹执行
main().catch((e) => {
  console.error("示例运行失败:", e)
  process.exitCode = 1
})

