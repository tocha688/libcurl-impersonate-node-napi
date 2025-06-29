import test from 'ava'

test('getVersion should return a valid version string', async (t) => {
  try {
    // 添加调试信息
    console.log('Current working directory:', process.cwd())
    console.log('Node.js architecture:', process.arch)
    console.log('Platform:', process.platform)
    
    // 尝试导入模块
    const { getVersion } = await import('../index.js')
    
    const version = getVersion()
    
    // 版本字符串应该是一个非空字符串
    t.true(typeof version === 'string')
    t.true(version.length > 0)
    
    // 版本格式通常应该是 x.y.z 或类似格式
    // 检查是否包含至少一个点号
    t.true(version.includes('curl'))
    
    // 打印版本信息以便于调试
    console.log(`Curl version: ${version}`)
  } catch (error) {
    console.error('Failed to load module:', error.message)
    console.error('Error code:', error.code)
    
    // 检查是否存在构建文件
    const fs = await import('fs')
    const path = await import('path')
    
    try {
      const files = fs.readdirSync(process.cwd())
      console.log('Files in current directory:', files.filter(f => f.includes('.node')))
    } catch (fsError) {
      console.error('Cannot list files:', fsError.message)
    }
    
    throw error
  }
})
