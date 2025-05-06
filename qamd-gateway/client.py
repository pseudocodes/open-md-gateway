#!/usr/bin/env python3
# encoding: utf-8
import websockets
import asyncio
import json
import logging
from datetime import datetime
from typing import List, Optional, Dict, Any, Callable
import signal
import os
import argparse

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class QAMDClient:
    """QAUTLRA-RS行情客户端，支持CTP、QQ和Sina三种行情源"""
    
    def __init__(self, 
                 ctp_url: str = "ws://localhost:8012/ws/market/ctp", 
                 qq_url: str = "ws://localhost:8012/ws/market/qq",
                 sina_url: str = "ws://localhost:8012/ws/market/sina"):
        """
        初始化行情客户端
        
        Args:
            ctp_url: CTP行情WebSocket URL
            qq_url: QQ行情WebSocket URL
            sina_url: 新浪行情WebSocket URL
        """
        self.ctp_url = ctp_url
        self.qq_url = qq_url
        self.sina_url = sina_url
        
        self.ctp_websocket: Optional[websockets.WebSocketClientProtocol] = None
        self.qq_websocket: Optional[websockets.WebSocketClientProtocol] = None
        self.sina_websocket: Optional[websockets.WebSocketClientProtocol] = None
        
        self.running = True
        self.subscribed_ctp_instruments: set = set()
        self.subscribed_qq_instruments: set = set()
        self.subscribed_sina_instruments: set = set()
        
        self.callbacks: Dict[str, List[Callable]] = {
            'ctp_market_data': [],
            'qq_market_data': [],
            'sina_market_data': [],
            'system': [],
            'error': [],
            'subscriptions': [],
        }
        
        # 设置信号处理器（用于优雅退出）
        signal.signal(signal.SIGINT, self._signal_handler2)
        signal.signal(signal.SIGTERM, self._signal_handler)

    def _signal_handler(self, signum, frame):
        """处理系统信号，实现优雅关闭"""
        logger.info(f"收到信号 {signum}，开始关闭...")
        self.running = False
        
    def _signal_handler2(self, signum, frame):
        """处理系统信号，实现优雅关闭"""
        logger.info(f"收到信号 INT: {signum}，开始关闭...")
        self.running = False
        os.exit(0)


    # 注册回调函数
    def on_ctp_market_data(self, callback: Callable[[Dict[str, Any]], None]):
        """注册CTP行情数据回调"""
        self.callbacks['ctp_market_data'].append(callback)

    def on_qq_market_data(self, callback: Callable[[Dict[str, Any]], None]):
        """注册QQ行情数据回调"""
        self.callbacks['qq_market_data'].append(callback)
        
    def on_sina_market_data(self, callback: Callable[[Dict[str, Any]], None]):
        """注册新浪行情数据回调"""
        self.callbacks['sina_market_data'].append(callback)

    def on_system_message(self, callback: Callable[[str], None]):
        """注册系统消息回调"""
        self.callbacks['system'].append(callback)

    def on_error(self, callback: Callable[[str], None]):
        """注册错误消息回调"""
        self.callbacks['error'].append(callback)

    def on_subscriptions(self, callback: Callable[[List[str]], None]):
        """注册订阅更新回调"""
        self.callbacks['subscriptions'].append(callback)

    # 连接函数
    async def connect_ctp(self):
        """连接到CTP行情服务器"""
        try:
            self.ctp_websocket = await websockets.connect(self.ctp_url)
            logger.info(f"已连接到CTP行情服务器: {self.ctp_url}")
            return True
        except Exception as e:
            logger.error(f"连接CTP行情服务器失败: {e}")
            return False

    async def connect_qq(self):
        """连接到QQ行情服务器"""
        try:
            self.qq_websocket = await websockets.connect(self.qq_url)
            logger.info(f"已连接到QQ行情服务器: {self.qq_url}")
            return True
        except Exception as e:
            logger.error(f"连接QQ行情服务器失败: {e}")
            return False
            
    async def connect_sina(self):
        """连接到新浪行情服务器"""
        try:
            self.sina_websocket = await websockets.connect(self.sina_url)
            logger.info(f"已连接到新浪行情服务器: {self.sina_url}")
            return True
        except Exception as e:
            logger.error(f"连接新浪行情服务器失败: {e}")
            return False

    async def disconnect(self):
        """断开所有WebSocket连接"""
        if self.ctp_websocket:
            await self.ctp_websocket.close()
            self.ctp_websocket = None
            logger.info("已断开CTP行情服务器连接")
            
        if self.qq_websocket:
            await self.qq_websocket.close()
            self.qq_websocket = None
            logger.info("已断开QQ行情服务器连接")
            
        if self.sina_websocket:
            await self.sina_websocket.close()
            self.sina_websocket = None
            logger.info("已断开新浪行情服务器连接")

    # 订阅函数
    async def subscribe_ctp(self, instruments: List[str]):
        """
        订阅CTP市场数据
        
        Args:
            instruments: 合约列表，格式如 ["SHFE.au2512", "CFFEX.IF2506"]
        """
        if not self.ctp_websocket:
            raise ConnectionError("未连接到CTP行情服务器")

        # 使用TradingView格式创建订阅消息
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(instruments)
        }
        
        await self.ctp_websocket.send(json.dumps(message))
        self.subscribed_ctp_instruments.update(instruments)
        logger.info(f"已订阅CTP合约: {instruments}")

    async def subscribe_qq(self, instruments: List[str]):
        """
        订阅QQ市场数据
        
        Args:
            instruments: 合约/股票代码列表，格式如 ["000001", "600000"]，支持灵活的代码格式
        """
        if not self.qq_websocket:
            raise ConnectionError("未连接到QQ行情服务器")

        # 使用TradingView格式创建订阅消息
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(instruments)
        }
        
        await self.qq_websocket.send(json.dumps(message))
        self.subscribed_qq_instruments.update(instruments)
        logger.info(f"已订阅QQ合约: {instruments}")
        
    async def subscribe_sina(self, instruments: List[str]):
        """
        订阅新浪市场数据
        
        Args:
            instruments: 合约/股票代码列表，格式如 ["600000", "AAPL"]，支持灵活的代码格式
        """
        if not self.sina_websocket:
            raise ConnectionError("未连接到新浪行情服务器")

        # 使用TradingView格式创建订阅消息
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(instruments)
        }
        
        await self.sina_websocket.send(json.dumps(message))
        self.subscribed_sina_instruments.update(instruments)
        logger.info(f"已订阅新浪合约: {instruments}")

    # 取消订阅函数
    async def unsubscribe_ctp(self, instruments: List[str]):
        """
        取消订阅CTP市场数据
        
        Args:
            instruments: 要取消订阅的合约列表
        """
        if not self.ctp_websocket:
            raise ConnectionError("未连接到CTP行情服务器")

        # 取消订阅时，我们发送除去要取消的合约的订阅列表
        all_instruments = list(self.subscribed_ctp_instruments)
        for ins in instruments:
            if ins in all_instruments:
                all_instruments.remove(ins)
        
        # 使用TradingView格式创建新的订阅消息
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(all_instruments)
        }
        
        await self.ctp_websocket.send(json.dumps(message))
        self.subscribed_ctp_instruments.difference_update(instruments)
        logger.info(f"已取消订阅CTP合约: {instruments}")
        
    async def unsubscribe_qq(self, instruments: List[str]):
        """
        取消订阅QQ市场数据
        
        Args:
            instruments: 要取消订阅的合约/股票代码列表
        """
        if not self.qq_websocket:
            raise ConnectionError("未连接到QQ行情服务器")

        all_instruments = list(self.subscribed_qq_instruments)
        for ins in instruments:
            if ins in all_instruments:
                all_instruments.remove(ins)
        
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(all_instruments)
        }
        
        await self.qq_websocket.send(json.dumps(message))
        self.subscribed_qq_instruments.difference_update(instruments)
        logger.info(f"已取消订阅QQ合约: {instruments}")
        
    async def unsubscribe_sina(self, instruments: List[str]):
        """
        取消订阅新浪市场数据
        
        Args:
            instruments: 要取消订阅的合约/股票代码列表
        """
        if not self.sina_websocket:
            raise ConnectionError("未连接到新浪行情服务器")

        all_instruments = list(self.subscribed_sina_instruments)
        for ins in instruments:
            if ins in all_instruments:
                all_instruments.remove(ins)
        
        message = {
            "aid": "subscribe_quote",
            "ins_list": ",".join(all_instruments)
        }
        
        await self.sina_websocket.send(json.dumps(message))
        self.subscribed_sina_instruments.difference_update(instruments)
        logger.info(f"已取消订阅新浪合约: {instruments}")
        
    # 查询当前订阅状态
    async def peek_ctp_subscriptions(self):
        """查询当前CTP订阅状态"""
        if not self.ctp_websocket:
            raise ConnectionError("未连接到CTP行情服务器")
            
        message = {
            "aid": "peek_message"
        }
        
        await self.ctp_websocket.send(json.dumps(message))
        logger.info("已请求CTP订阅状态")
        
    async def peek_qq_subscriptions(self):
        """查询当前QQ订阅状态"""
        if not self.qq_websocket:
            raise ConnectionError("未连接到QQ行情服务器")
            
        message = {
            "aid": "peek_message"
        }
        
        await self.qq_websocket.send(json.dumps(message))
        logger.info("已请求QQ订阅状态")
        
    async def peek_sina_subscriptions(self):
        """查询当前新浪订阅状态"""
        if not self.sina_websocket:
            raise ConnectionError("未连接到新浪行情服务器")
            
        message = {
            "aid": "peek_message"
        }
        
        await self.sina_websocket.send(json.dumps(message))
        logger.info("已请求新浪订阅状态")

    # 消息处理函数
    async def _handle_message(self, message: str, source_type: str):
        """
        处理接收到的WebSocket消息
        
        Args:
            message: 收到的消息内容
            source_type: 消息来源类型，可以是 'ctp', 'qq' 或 'sina'
        """
        try:
            data = json.loads(message)
            print(f"收到{source_type}消息: {data}")
            logger.debug(f"收到{source_type}消息: {data}")
            
            if isinstance(data, dict):
                # 处理TradingView格式消息（带有aid字段）
                if 'aid' in data:
                    aid_type = data.get('aid')
                    
                    if aid_type == 'rtn_data':
                        # 处理行情数据
                        if 'data' in data:
                            callback_key = f'{source_type}_market_data'
                            for callback in self.callbacks.get(callback_key, []):
                                await asyncio.create_task(callback(data['data']))
                    elif aid_type == 'peek_message':
                        # 处理订阅状态消息
                        logger.info(f"当前{source_type}订阅: {data.get('ins_list', '')}")
                        instruments = data.get('ins_list', '').split(',') if data.get('ins_list') else []
                        for callback in self.callbacks['subscriptions']:
                            await asyncio.create_task(callback(instruments))
                    else:
                        logger.info(f"收到{source_type}消息, aid: {aid_type}")
                
                # 处理旧格式消息（带有type字段）
                elif 'type' in data:
                    msg_type = data['type']
                    payload = data.get('payload', {})
                    
                    if msg_type == 'market_data':
                        callback_key = f'{source_type}_market_data'
                        for callback in self.callbacks.get(callback_key, []):
                            await asyncio.create_task(callback(payload['data']))
                    elif msg_type == 'system':
                        for callback in self.callbacks['system']:
                            await asyncio.create_task(callback(payload['message']))
                    elif msg_type == 'error':
                        for callback in self.callbacks['error']:
                            await asyncio.create_task(callback(payload['message']))
                    elif msg_type == 'subscriptions':
                        for callback in self.callbacks['subscriptions']:
                            await asyncio.create_task(callback(payload['instruments']))
                    elif msg_type == 'pong':
                        logger.debug(f"收到{source_type}pong响应")
                else:
                    logger.warning(f"收到{source_type}消息，但没有aid或type字段: {data}")
            else:
                logger.warning(f"收到{source_type}非dict类型消息: {data}")
                
        except json.JSONDecodeError as e:
            logger.error(f"解析{source_type}消息失败: {e}")
        except Exception as e:
            logger.error(f"处理{source_type}消息出错: {e}")

    # 心跳函数
    async def _heartbeat(self, websocket, source_type: str):
        """
        定期发送心跳消息以保持连接
        
        Args:
            websocket: WebSocket连接
            source_type: 连接类型，可以是 'ctp', 'qq' 或 'sina'
        """
        while self.running and websocket:
            try:
                message = {"type": "ping"}
                await websocket.send(json.dumps(message))
                logger.debug(f"发送{source_type}ping")
                await asyncio.sleep(10)  # 每10秒发送一次心跳
            except Exception as e:
                logger.error(f"{source_type}心跳发送出错: {e}")
                break

    # 连接运行函数
    async def _run_connection(self, source_type: str):
        """
        运行单个WebSocket连接
        
        Args:
            source_type: 连接类型，可以是 'ctp', 'qq' 或 'sina'
        """
        # 获取对应的WebSocket连接和连接函数
        if source_type == 'ctp':
            websocket = self.ctp_websocket
            connect_func = self.connect_ctp
        elif source_type == 'qq':
            websocket = self.qq_websocket
            connect_func = self.connect_qq
        elif source_type == 'sina':
            websocket = self.sina_websocket
            connect_func = self.connect_sina
        else:
            logger.error(f"未知的连接类型: {source_type}")
            return
        
        while self.running:
            try:
                print(f'here ?? {websocket}')
                if not websocket:
                    if not await connect_func():
                        await asyncio.sleep(5)  # 连接失败，等待后重试
                        continue
                    
                    # 更新WebSocket连接引用
                    if source_type == 'ctp':
                        websocket = self.ctp_websocket
                    elif source_type == 'qq':
                        websocket = self.qq_websocket
                    elif source_type == 'sina':
                        websocket = self.sina_websocket

                # 启动心跳任务
                heartbeat_task = asyncio.create_task(self._heartbeat(websocket, source_type))

                # 主消息循环
                async for message in websocket:
                    if not self.running:
                        print("break ????!")
                        break
                    await self._handle_message(message, source_type)

                # 清理心跳任务
                heartbeat_task.cancel()
                try:
                    await heartbeat_task
                except asyncio.CancelledError:
                    pass

            except websockets.exceptions.ConnectionClosed:
                logger.warning(f"{source_type}连接已关闭，尝试重连...")
                await asyncio.sleep(5)
                websocket = None 
            except Exception as e:
                logger.error(f"{source_type}运行循环出错: {e}")
                await asyncio.sleep(5)

    async def run(self, enable_ctp=True, enable_qq=True, enable_sina=True):
        """
        主运行循环
        
        Args:
            enable_ctp: 是否启用CTP行情连接
            enable_qq: 是否启用QQ行情连接
            enable_sina: 是否启用新浪行情连接
        """
        # 创建连接任务
        tasks = []
        
        if enable_ctp:
            tasks.append(asyncio.create_task(self._run_connection('ctp')))
            
        if enable_qq:
            tasks.append(asyncio.create_task(self._run_connection('qq')))
            
        if enable_sina:
            tasks.append(asyncio.create_task(self._run_connection('sina')))
        
        # 等待所有任务完成
        try:
            await asyncio.gather(*tasks)
        finally:
            # 清理
            await self.disconnect()

# 示例回调函数
async def example_ctp_market_data_callback(data: Dict[str, Any]):
    print(f"CTP行情数据: {data}")

async def example_qq_market_data_callback(data: Dict[str, Any]):
    print(f"QQ行情数据: {data}")

async def example_sina_market_data_callback(data: Dict[str, Any]):
    print(f"新浪行情数据: {data}")

async def example_system_callback(message: str):
    print(f"系统消息: {message}")

async def example_error_callback(message: str):
    print(f"错误消息: {message}")

async def main():
    # 命令行参数解析
    parser = argparse.ArgumentParser(description='QAUTLRA-RS行情客户端')
    parser.add_argument('--ctp', action='store_true', help='启用CTP行情')
    parser.add_argument('--qq', action='store_true', help='启用QQ行情')
    parser.add_argument('--sina', action='store_true', help='启用新浪行情')
    parser.add_argument('--all', action='store_true', help='启用所有行情源')
    args = parser.parse_args()
    
    # 如果没有指定任何参数，默认启用所有行情源
    if not (args.ctp or args.qq or args.sina or args.all):
        args.all = True
    
    # 创建客户端实例
    client = QAMDClient()
    
    # 注册回调
    client.on_ctp_market_data(example_ctp_market_data_callback)
    client.on_qq_market_data(example_qq_market_data_callback)
    client.on_sina_market_data(example_sina_market_data_callback)
    client.on_system_message(example_system_callback)
    client.on_error(example_error_callback)
    
    # 创建运行客户端的任务
    enable_ctp = args.ctp or args.all
    enable_qq = args.qq or args.all
    enable_sina = args.sina or args.all
    
    client_task = asyncio.create_task(client.run(
        enable_ctp=enable_ctp,
        enable_qq=enable_qq,
        enable_sina=enable_sina
    ))
    
    # 等待客户端连接
    await asyncio.sleep(2)
    
    try:
        # 订阅不同市场的行情数据
        if enable_ctp:
            await client.subscribe_ctp(["au2506", "IF2506", "rb2512"])
            await client.peek_ctp_subscriptions()
        
        if enable_qq:
            # QQ行情支持灵活的代码格式
            await client.subscribe_qq([ "000001", "00700"])
            await client.peek_qq_subscriptions()
            
        if enable_sina:
            # 新浪行情也支持灵活的代码格式
            await client.subscribe_sina(["SSE.600000", "HKEX.00700"])
            await client.peek_sina_subscriptions()
        
        # 运行一段时间
        print("\n按 Ctrl+C 退出")
        while True:
            await asyncio.sleep(1)
        
    except KeyboardInterrupt:
        print("接收到退出信号")
    finally:
        # 停止客户端
        client.running = False
        await client_task

if __name__ == "__main__":
    asyncio.run(main()) 