- 14.74g 压缩时间 2193秒 压缩之后大小8.83G

- 14.74g 压缩时间 911秒 压缩之后大小8.83g

- 14.74g 压缩时间 641秒 压缩之后大小8.64G




pwsh -Command "$env:RUST_LOG='debug'; Measure-Command { .\target\release\zip1.exe -t 8 ./dist output.zip }"