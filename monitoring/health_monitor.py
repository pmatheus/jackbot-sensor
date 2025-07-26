#!/usr/bin/env python3
"""
Health Monitor for Jackbot Sensors
Monitors sensor health, performance metrics, and sends alerts
"""

import os
import time
import json
import logging
import asyncio
import aiohttp
import redis.asyncio as redis
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('health-monitor')

@dataclass
class SensorHealth:
    """Sensor health status"""
    instance_id: str
    endpoint: str
    healthy: bool
    latency_ms: float
    last_check: str
    error: Optional[str] = None
    metrics: Optional[Dict[str, Any]] = None

@dataclass
class HealthAlert:
    """Health alert notification"""
    sensor_id: str
    level: str  # 'warning', 'critical'
    message: str
    timestamp: str
    metrics: Optional[Dict[str, Any]] = None

class HealthMonitor:
    """Monitors sensor health and performance"""
    
    def __init__(self):
        self.sensor_endpoints = os.getenv('SENSOR_ENDPOINTS', '').split(',')
        self.check_interval = int(os.getenv('CHECK_INTERVAL', '10'))
        self.alert_webhook = os.getenv('ALERT_WEBHOOK')
        self.redis_url = os.getenv('REDIS_URL', 'redis://localhost:6379')
        
        # Thresholds
        self.latency_warning_ms = 100
        self.latency_critical_ms = 500
        self.failure_threshold = 3
        
        # State tracking
        self.failure_counts: Dict[str, int] = {}
        self.last_alert: Dict[str, datetime] = {}
        self.alert_cooldown = timedelta(minutes=5)
        
    async def start(self):
        """Start health monitoring"""
        logger.info(f"Starting health monitor for {len(self.sensor_endpoints)} sensors")
        
        # Initialize Redis connection
        self.redis = await redis.from_url(self.redis_url)
        
        # Start monitoring loop
        while True:
            try:
                await self.check_all_sensors()
                await asyncio.sleep(self.check_interval)
            except Exception as e:
                logger.error(f"Error in monitoring loop: {e}")
                await asyncio.sleep(self.check_interval)
    
    async def check_all_sensors(self):
        """Check health of all sensors"""
        async with aiohttp.ClientSession() as session:
            tasks = [
                self.check_sensor_health(session, endpoint)
                for endpoint in self.sensor_endpoints
            ]
            results = await asyncio.gather(*tasks, return_exceptions=True)
            
            # Process results
            for endpoint, result in zip(self.sensor_endpoints, results):
                if isinstance(result, Exception):
                    logger.error(f"Failed to check {endpoint}: {result}")
                    await self.handle_sensor_failure(endpoint, str(result))
                else:
                    await self.process_health_status(result)
    
    async def check_sensor_health(self, session: aiohttp.ClientSession, endpoint: str) -> SensorHealth:
        """Check individual sensor health"""
        start_time = time.time()
        
        try:
            # Call health endpoint
            async with session.get(f"{endpoint}/health", timeout=3) as response:
                latency_ms = (time.time() - start_time) * 1000
                
                if response.status == 200:
                    data = await response.json()
                    
                    return SensorHealth(
                        instance_id=data.get('instance_id', 'unknown'),
                        endpoint=endpoint,
                        healthy=True,
                        latency_ms=latency_ms,
                        last_check=datetime.utcnow().isoformat(),
                        metrics=data.get('metrics', {})
                    )
                else:
                    raise Exception(f"Health check returned {response.status}")
                    
        except asyncio.TimeoutError:
            latency_ms = (time.time() - start_time) * 1000
            return SensorHealth(
                instance_id='unknown',
                endpoint=endpoint,
                healthy=False,
                latency_ms=latency_ms,
                last_check=datetime.utcnow().isoformat(),
                error="Health check timeout"
            )
        except Exception as e:
            latency_ms = (time.time() - start_time) * 1000
            return SensorHealth(
                instance_id='unknown',
                endpoint=endpoint,
                healthy=False,
                latency_ms=latency_ms,
                last_check=datetime.utcnow().isoformat(),
                error=str(e)
            )
    
    async def process_health_status(self, health: SensorHealth):
        """Process health status and trigger alerts if needed"""
        # Store in Redis
        await self.store_health_status(health)
        
        # Check for issues
        if not health.healthy:
            await self.handle_sensor_failure(health.endpoint, health.error)
        else:
            # Reset failure count on success
            self.failure_counts[health.endpoint] = 0
            
            # Check latency
            if health.latency_ms > self.latency_critical_ms:
                await self.send_alert(HealthAlert(
                    sensor_id=health.instance_id,
                    level='critical',
                    message=f"Critical latency: {health.latency_ms:.1f}ms",
                    timestamp=datetime.utcnow().isoformat(),
                    metrics={'latency_ms': health.latency_ms}
                ))
            elif health.latency_ms > self.latency_warning_ms:
                await self.send_alert(HealthAlert(
                    sensor_id=health.instance_id,
                    level='warning',
                    message=f"High latency: {health.latency_ms:.1f}ms",
                    timestamp=datetime.utcnow().isoformat(),
                    metrics={'latency_ms': health.latency_ms}
                ))
            
            # Check metrics
            if health.metrics:
                await self.check_performance_metrics(health)
    
    async def handle_sensor_failure(self, endpoint: str, error: str):
        """Handle sensor failure"""
        # Increment failure count
        self.failure_counts[endpoint] = self.failure_counts.get(endpoint, 0) + 1
        
        # Check if we should alert
        if self.failure_counts[endpoint] >= self.failure_threshold:
            await self.send_alert(HealthAlert(
                sensor_id=endpoint,
                level='critical',
                message=f"Sensor down after {self.failure_counts[endpoint]} failures: {error}",
                timestamp=datetime.utcnow().isoformat(),
                metrics={'failure_count': self.failure_counts[endpoint]}
            ))
    
    async def check_performance_metrics(self, health: SensorHealth):
        """Check performance metrics for issues"""
        metrics = health.metrics
        
        # Check CPU usage
        if metrics.get('cpu_percent', 0) > 80:
            await self.send_alert(HealthAlert(
                sensor_id=health.instance_id,
                level='warning' if metrics['cpu_percent'] < 90 else 'critical',
                message=f"High CPU usage: {metrics['cpu_percent']}%",
                timestamp=datetime.utcnow().isoformat(),
                metrics={'cpu_percent': metrics['cpu_percent']}
            ))
        
        # Check memory usage
        if metrics.get('memory_percent', 0) > 80:
            await self.send_alert(HealthAlert(
                sensor_id=health.instance_id,
                level='warning' if metrics['memory_percent'] < 90 else 'critical',
                message=f"High memory usage: {metrics['memory_percent']}%",
                timestamp=datetime.utcnow().isoformat(),
                metrics={'memory_percent': metrics['memory_percent']}
            ))
        
        # Check message lag
        if metrics.get('kafka_lag', 0) > 1000:
            await self.send_alert(HealthAlert(
                sensor_id=health.instance_id,
                level='warning' if metrics['kafka_lag'] < 5000 else 'critical',
                message=f"High Kafka lag: {metrics['kafka_lag']} messages",
                timestamp=datetime.utcnow().isoformat(),
                metrics={'kafka_lag': metrics['kafka_lag']}
            ))
    
    async def store_health_status(self, health: SensorHealth):
        """Store health status in Redis"""
        try:
            # Store current status
            key = f"sensor:health:{health.instance_id}"
            await self.redis.setex(
                key,
                300,  # 5 minute expiry
                json.dumps(asdict(health))
            )
            
            # Store in time series for history
            ts_key = f"sensor:health:history:{health.instance_id}"
            await self.redis.zadd(
                ts_key,
                {json.dumps(asdict(health)): time.time()}
            )
            
            # Trim history to last 24 hours
            cutoff = time.time() - (24 * 60 * 60)
            await self.redis.zremrangebyscore(ts_key, 0, cutoff)
            
        except Exception as e:
            logger.error(f"Failed to store health status: {e}")
    
    async def send_alert(self, alert: HealthAlert):
        """Send alert notification"""
        # Check cooldown
        last_alert_time = self.last_alert.get(f"{alert.sensor_id}:{alert.message}", datetime.min)
        if datetime.utcnow() - last_alert_time < self.alert_cooldown:
            return
        
        # Update last alert time
        self.last_alert[f"{alert.sensor_id}:{alert.message}"] = datetime.utcnow()
        
        # Log alert
        logger.warning(f"Alert: {alert.level} - {alert.sensor_id} - {alert.message}")
        
        # Send webhook if configured
        if self.alert_webhook:
            try:
                async with aiohttp.ClientSession() as session:
                    await session.post(
                        self.alert_webhook,
                        json=asdict(alert),
                        timeout=5
                    )
            except Exception as e:
                logger.error(f"Failed to send webhook alert: {e}")
        
        # Store alert in Redis
        try:
            alert_key = f"sensor:alerts:{alert.sensor_id}"
            await self.redis.lpush(alert_key, json.dumps(asdict(alert)))
            await self.redis.ltrim(alert_key, 0, 99)  # Keep last 100 alerts
        except Exception as e:
            logger.error(f"Failed to store alert: {e}")

async def main():
    """Main entry point"""
    monitor = HealthMonitor()
    await monitor.start()

if __name__ == "__main__":
    asyncio.run(main())