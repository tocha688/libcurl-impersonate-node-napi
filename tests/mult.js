const { setLibPath, getLibPath, Curl, CurlOpt, globalInit, CurlInfo, CurlMulti, CurlMOpt } = require("..")
const path = require("path")

setLibPath(path.join(process.cwd(), `/libs/x86_64-win32/bin/libcurl.dll`))

console.log("lib路径", getLibPath())

globalInit(3)

const curl = new Curl()
curl.setOptString(CurlOpt.Url, "https://tls.peet.ws/api/all")
curl.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl.setOptLong(CurlOpt.SslVerifyHost, 0)
curl.setOptLong(CurlOpt.Verbose, 1)
curl.impersonate("chrome136", true)

const curl2 = new Curl()
curl2.setOptString(CurlOpt.Url, "https://tls.peet.ws/api/all")
curl2.setOptLong(CurlOpt.SslVerifyPeer, 0)
curl2.setOptLong(CurlOpt.SslVerifyHost, 0)
curl2.setOptLong(CurlOpt.Verbose, 1)
curl2.impersonate("chrome136", true)

// curl.init()

const multi = new CurlMulti()
multi.perform(curl, (...args) => {
    console.log("Request 1 completed!", ...args)
}, (...args) => {
    console.log("Request 1 error!", ...args)
})
console.log("Request 1 started!")

multi.perform(curl2, (...args) => {
    console.log("Request 2 completed!", ...args)
}, (...args) => {
    console.log("Request 2 error!", ...args)
})

console.log("Request 2 started!")



