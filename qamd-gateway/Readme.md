# QAMD Gateway

来自 qautral-rs 的 qamdgateway 的 ctp2rs 重构实现，通过配置信息就可以简化原有的复杂依赖和构建
该 demo 还在开发中，请勿在生产环境使用

## Features

- 支持多个市场数据源： 
  - CTP（中国金融期货交易所） 
  - 腾讯财经 
  - 新浪财经 
- 将市场数据转换为统一的 QAMD MDSnapshot 格式 
- 通过 WebSocket 进行实时市场数据分发 
- 通过 RESTful API 进行订阅管理 
- 基于 Actor 的架构，支持高并发和容错 
- 可配置的经纪商连接 
- 支持多种合约订阅 
- **增量市场数据更新以优化性能** 
  - 首次连接接收完整快照 
  - 后续更新仅包含更改字段
  - 批量处理更新以减少网络流量

## Architecture

```
                                  +-------------------+
                                  |                   |
                                  |  Market Data      |
                                  |  Distributor      |
                                  |  (Actor)          |
                                  |                   |
                                  +-------------------+
                                    ^      |
                                    |      v
+-----------------+  +---------------+  +------------------+
|                 |  |               |  |                  |
| CTP Market Data |->| Market Data   |<-| WebSocket       |
| Source (Actor)  |  | Connector     |  | Clients         |
|                 |  | (Actor)       |  |                  |
+-----------------+  +---------------+  +------------------+
                        ^     ^
+-----------------+     |     |     +------------------+
|                 |     |     |     |                  |
| QQ Market Data  |---->|     |<----| REST API         |
| Source (Actor)  |           |     | Clients          |
|                 |           |     |                  |
+-----------------+           |     +------------------+
                              |
+-----------------+           |
|                 |           |
| Sina Market Data|---------->|
| Source (Actor)  |
|                 |
+-----------------+
```



## Installation  

1. Clone the repository:
   ```bash
   git clone https://github.com/pseudocodes/open-md-gateway.git
   cd open-md-gateway/qamd-gateway
   ```

2. Build the QAMD Gateway with your desired market data sources:
   ```bash
   # Build with all market data sources
   cargo build --release --package qamd-gateway
   ```

3. Update the `config.json` with your broker information.

4. Run the gateway:
   ```bash
   ./target/release/qamd-gateway
   ```

## Configuration

The gateway is configured using a `config.json` file. Example:
网关使用 `config.json` 文件进行配置。示例：

```json
{
  "brokers": {
    "testbroker": {
      "name": "Test Broker",
      "front_addr": "tcp://180.166.103.21:57213",
      "user_id": "",
      "password": "",
      "broker_id": ""
    },
    "qqbroker": {
      "name": "QQ Finance",
      "front_addr": "tcp://quotes.qq.com/path",
      "user_id": "",
      "password": "",
      "broker_id": "qq"
    },
    "sinabroker": {
      "name": "Sina Finance",
      "front_addr": "tcp://hq.sinajs.cn/path",
      "user_id": "",
      "password": "",
      "broker_id": "sina"
    }
  },
  "default_broker": "testbroker",
  "websocket": {
    "host": "0.0.0.0",
    "port": 8081,
    "path": "/ws/market"
  },
  "rest_api": {
    "host": "0.0.0.0",
    "port": 8080,
    "cors": {
      "allow_all": true,
      "allowed_origins": [],
      "allow_credentials": true
    }
  },
  "subscription": {
    "default_instruments": [
      "au2512",
      "rb2512"
    ]
  },
  "incremental_updates": {
    "enabled": true,
    "batch_interval_ms": 100,
    "batch_size_threshold": 50
  }
}
```

## Actor System 

网关使用基于 actor 的架构以实现高并发和容错性：

- **MarketDataActor**: 表示特定市场数据源（CTP、QQ、 sina）的连接 
- **MarketDataConnector**: 管理多个市场数据源并将数据从源转发给分发器 
- **MarketDataDistributor**: 将市场数据分发给订阅的客户端 
- **WebSocket Sessions**: 管理客户端连接和订阅

## API Usage

### REST API

#### 获取订阅 
```
GET /api/subscriptions
```

#### 订阅合约
```
POST /api/subscribe
Content-Type: application/json 
{
  "instruments": ["au2412", "rb2412"]
}
```

#### 取消订阅合约 
```
POST /api/unsubscribe
Content-Type: application/json
{
  "instruments": ["au2412"]
}
```

### WebSocket API

Connect to WebSocket endpoint:

```
ws://localhost:8081/ws/market
```

#### 订阅消息 
```json
{
  "type": "subscribe",
  "payload": {
    "instruments": ["au2412", "rb2412"]
  }
}
```

#### 取消订阅消息
```json
{
  "type": "unsubscribe",
  "payload": {
    "instruments": ["au2412"]
  }
}
```

#### 获取订阅
```json
{
  "type": "subscriptions"
}
```

#### 市场数据消息 (接收)
```json
{
  "type": "market_data",
  "payload": {
    "data": {
      "instrument_id": "SHFE_au2512",
      "last_price": 799.0,
      "source": "CTP",  // or "QQ" or "Sina"
      // Other fields...
    }
  }
}
```
） 

#### TradingView Format (New)
```json
{
  "aid": "rtn_data",
  "data": [
    {
      "quotes": {
        "SHFE_au2412": {
          "instrument_id": "SHFE_au2412",
          "datetime": "2023-05-01 14:30:25.123",
          "last_price": 400.5,
          "volume": 12345,
          "amount": 4950000.0
          // Additional fields depending on update type
        }
      }
    }
  ]
}
```

## Incremental Market Data Updates

网关现在支持增量市场数据更新，显著减少了带宽使用并提高了性能：

### How It Works

1. **首次连接**：
   - 当客户端首次连接或订阅某个交易品种时，会收到一个完整快照 
   - - 该快照包含该交易品种的所有可用字段

2. **后续更新**：
   - 只发送自上次更新以来发生变化的字段
   - 每次更新都包括`instrument_id`和发生变化的字段

3. **批量处理**：
   - 将更新批量处理以减少消息频率 
   - 配置选项控制批量间隔和大小阈值 
   - 可将多个合约的更新合并为单个消息

### Benefits  

- **减少网络流量**：与完整快照相比，最多可减少 90%的数据传输 
- **降低延迟**：较小的消息对服务器和客户端处理更快 
- **更高的吞吐量**：支持更多的并发客户端和合约 
- **提高可扩展性**：减少服务器 CPU 和内存使用

### Implementation Details

- 服务器为每个客户端维护最新数据的快照 
- 客户端 SDK 自动将增量更新合并为完整的视图 
- 既兼容 TradingView 格式也兼容旧格式

## License

Apache License 