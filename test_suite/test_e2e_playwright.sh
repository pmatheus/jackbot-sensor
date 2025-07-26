#!/bin/bash
LOG_DIR="$1"
FRONTEND_URL="${FRONTEND_URL:-http://localhost:3000}"

echo "=== Testing E2E with Playwright ===" | tee "$LOG_DIR/e2e_summary.txt"
echo "Frontend URL: $FRONTEND_URL" | tee -a "$LOG_DIR/e2e_summary.txt"

# Create Playwright test suite
cat > "$LOG_DIR/e2e_test_suite.js" << 'EOF'
const { chromium } = require('playwright');
const fs = require('fs');

const frontendUrl = process.env.FRONTEND_URL || 'http://localhost:3000';
const logFile = process.env.LOG_FILE || 'e2e_results.json';

const results = {
    timestamp: new Date().toISOString(),
    url: frontendUrl,
    tests: {
        homepage: { passed: false, error: null, duration: 0 },
        marketData: { passed: false, error: null, duration: 0 },
        portfolio: { passed: false, error: null, duration: 0 },
        orderFlow: { passed: false, error: null, duration: 0 }
    },
    performance: {
        pageLoadTime: 0,
        fps: 0
    },
    screenshots: []
};

async function runTests() {
    const browser = await chromium.launch({ 
        headless: true,
        args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    const context = await browser.newContext({
        viewport: { width: 1920, height: 1080 }
    });
    
    const page = await context.newPage();
    
    // Enable console logging
    page.on('console', msg => console.log('Browser console:', msg.text()));
    page.on('pageerror', error => console.error('Page error:', error));
    
    try {
        // Test 1: Homepage Load
        console.log('\n[Test 1] Testing homepage load...');
        const testStart = Date.now();
        
        try {
            await page.goto(frontendUrl, { 
                waitUntil: 'networkidle',
                timeout: 30000 
            });
            
            // Measure page load performance
            const perfTiming = await page.evaluate(() => {
                const timing = performance.timing;
                return {
                    loadTime: timing.loadEventEnd - timing.navigationStart,
                    domReady: timing.domContentLoadedEventEnd - timing.navigationStart,
                    firstPaint: performance.getEntriesByType('paint')[0]?.startTime || 0
                };
            });
            
            results.performance.pageLoadTime = perfTiming.loadTime;
            results.tests.homepage.passed = true;
            results.tests.homepage.duration = Date.now() - testStart;
            
            console.log(`✓ Homepage loaded in ${perfTiming.loadTime}ms`);
            
            // Take screenshot
            await page.screenshot({ 
                path: `${process.env.LOG_DIR}/homepage.png`,
                fullPage: true 
            });
            results.screenshots.push('homepage.png');
            
        } catch (error) {
            results.tests.homepage.error = error.message;
            console.error('✗ Homepage test failed:', error.message);
        }
        
        // Test 2: Market Data Display
        console.log('\n[Test 2] Testing market data display...');
        const marketTestStart = Date.now();
        
        try {
            // Look for market data elements
            const marketDataSelectors = [
                '[data-testid="market-data"]',
                '.market-data',
                '#market-data',
                '[class*="market"]',
                '[class*="price"]',
                'text=/BTC|ETH|USD/'
            ];
            
            let marketDataFound = false;
            for (const selector of marketDataSelectors) {
                try {
                    await page.waitForSelector(selector, { timeout: 5000 });
                    marketDataFound = true;
                    console.log(`Found market data with selector: ${selector}`);
                    break;
                } catch (e) {
                    // Continue to next selector
                }
            }
            
            if (marketDataFound) {
                // Wait for price updates
                await page.waitForTimeout(3000);
                
                // Check for WebSocket connection
                const wsConnected = await page.evaluate(() => {
                    return window.WebSocket !== undefined;
                });
                
                results.tests.marketData.passed = true;
                console.log('✓ Market data display found');
                
                // Take screenshot
                await page.screenshot({ 
                    path: `${process.env.LOG_DIR}/market_data.png` 
                });
                results.screenshots.push('market_data.png');
            } else {
                throw new Error('Market data elements not found');
            }
            
            results.tests.marketData.duration = Date.now() - marketTestStart;
            
        } catch (error) {
            results.tests.marketData.error = error.message;
            console.error('✗ Market data test failed:', error.message);
        }
        
        // Test 3: Portfolio View
        console.log('\n[Test 3] Testing portfolio view...');
        const portfolioTestStart = Date.now();
        
        try {
            // Navigate to portfolio if not on main page
            const portfolioSelectors = [
                '[data-testid="portfolio"]',
                'a[href*="portfolio"]',
                'button:has-text("Portfolio")',
                'text=/Portfolio|Account|Balance/'
            ];
            
            for (const selector of portfolioSelectors) {
                try {
                    await page.click(selector, { timeout: 5000 });
                    await page.waitForTimeout(2000);
                    break;
                } catch (e) {
                    // Continue
                }
            }
            
            // Look for portfolio elements
            const balanceSelectors = [
                '[data-testid="balance"]',
                '.balance',
                '[class*="balance"]',
                'text=/Balance|USD|\\$/'
            ];
            
            let portfolioFound = false;
            for (const selector of balanceSelectors) {
                try {
                    await page.waitForSelector(selector, { timeout: 5000 });
                    portfolioFound = true;
                    console.log(`Found portfolio with selector: ${selector}`);
                    break;
                } catch (e) {
                    // Continue
                }
            }
            
            if (portfolioFound) {
                results.tests.portfolio.passed = true;
                console.log('✓ Portfolio view found');
                
                await page.screenshot({ 
                    path: `${process.env.LOG_DIR}/portfolio.png` 
                });
                results.screenshots.push('portfolio.png');
            }
            
            results.tests.portfolio.duration = Date.now() - portfolioTestStart;
            
        } catch (error) {
            results.tests.portfolio.error = error.message;
            console.error('✗ Portfolio test failed:', error.message);
        }
        
        // Test 4: Order Flow
        console.log('\n[Test 4] Testing order flow...');
        const orderTestStart = Date.now();
        
        try {
            // Look for order placement UI
            const orderSelectors = [
                '[data-testid="place-order"]',
                'button:has-text("Buy")',
                'button:has-text("Sell")',
                '[class*="order"]',
                'text=/Buy|Sell|Order/'
            ];
            
            let orderUIFound = false;
            for (const selector of orderSelectors) {
                try {
                    const element = await page.waitForSelector(selector, { timeout: 5000 });
                    if (element) {
                        orderUIFound = true;
                        console.log(`Found order UI with selector: ${selector}`);
                        
                        // Try to interact with order form
                        const inputSelectors = [
                            'input[name="quantity"]',
                            'input[name*="amount"]',
                            'input[type="number"]',
                            '[data-testid="quantity-input"]'
                        ];
                        
                        for (const inputSelector of inputSelectors) {
                            try {
                                await page.fill(inputSelector, '0.001', { timeout: 3000 });
                                console.log('✓ Filled order quantity');
                                break;
                            } catch (e) {
                                // Continue
                            }
                        }
                        
                        break;
                    }
                } catch (e) {
                    // Continue
                }
            }
            
            if (orderUIFound) {
                results.tests.orderFlow.passed = true;
                console.log('✓ Order flow UI found');
                
                await page.screenshot({ 
                    path: `${process.env.LOG_DIR}/order_flow.png` 
                });
                results.screenshots.push('order_flow.png');
            }
            
            results.tests.orderFlow.duration = Date.now() - orderTestStart;
            
        } catch (error) {
            results.tests.orderFlow.error = error.message;
            console.error('✗ Order flow test failed:', error.message);
        }
        
        // Measure FPS
        console.log('\n[Performance] Measuring FPS...');
        try {
            const fps = await page.evaluate(() => {
                return new Promise(resolve => {
                    let frames = 0;
                    const startTime = performance.now();
                    
                    function countFrames() {
                        frames++;
                        if (performance.now() - startTime < 1000) {
                            requestAnimationFrame(countFrames);
                        } else {
                            resolve(frames);
                        }
                    }
                    
                    requestAnimationFrame(countFrames);
                });
            });
            
            results.performance.fps = fps;
            console.log(`FPS measured: ${fps}`);
            
        } catch (error) {
            console.error('FPS measurement failed:', error.message);
        }
        
    } catch (error) {
        console.error('Test suite error:', error);
    } finally {
        await browser.close();
    }
    
    // Calculate summary
    const passedTests = Object.values(results.tests).filter(t => t.passed).length;
    const totalTests = Object.keys(results.tests).length;
    results.summary = {
        passed: passedTests,
        failed: totalTests - passedTests,
        total: totalTests,
        successRate: (passedTests / totalTests) * 100
    };
    
    // Save results
    fs.writeFileSync(logFile, JSON.stringify(results, null, 2));
    
    console.log('\n=== E2E Test Summary ===');
    console.log(`Tests Passed: ${passedTests}/${totalTests} (${results.summary.successRate}%)`);
    console.log(`Page Load Time: ${results.performance.pageLoadTime}ms`);
    console.log(`FPS: ${results.performance.fps}`);
}

// Run tests
runTests().catch(error => {
    console.error('Fatal error:', error);
    process.exit(1);
});
EOF

# Check if Playwright is available
if [ -d "/Users/user/jackbot/jackbot-terminal/node_modules/playwright" ]; then
    echo "Using existing Playwright installation" | tee -a "$LOG_DIR/e2e_summary.txt"
    
    # Run Playwright tests
    cd /Users/user/jackbot/jackbot-terminal
    FRONTEND_URL="$FRONTEND_URL" LOG_FILE="$LOG_DIR/e2e_results.json" LOG_DIR="$LOG_DIR" \
        timeout 120 node "$LOG_DIR/e2e_test_suite.js" 2>&1 | tee "$LOG_DIR/e2e_test.log"
    
    cd - > /dev/null
else
    echo "Playwright not found, attempting basic browser test..." | tee -a "$LOG_DIR/e2e_summary.txt"
    
    # Fallback to curl-based testing
    echo -e "\nFalling back to HTTP-based testing..." | tee -a "$LOG_DIR/e2e_summary.txt"
    
    # Test frontend availability
    RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "$FRONTEND_URL" 2>/dev/null || echo "000")
    echo "Frontend HTTP Status: $RESPONSE" | tee -a "$LOG_DIR/e2e_summary.txt"
    
    if [ "$RESPONSE" = "200" ]; then
        # Get page content
        curl -s "$FRONTEND_URL" > "$LOG_DIR/frontend_page.html"
        
        # Check for key elements
        grep -q "market\|price\|portfolio\|order" "$LOG_DIR/frontend_page.html" && \
            echo "✓ Found trading-related content" | tee -a "$LOG_DIR/e2e_summary.txt" || \
            echo "✗ No trading content found" | tee -a "$LOG_DIR/e2e_summary.txt"
    fi
    
    # Create minimal results
    cat > "$LOG_DIR/e2e_results.json" << EOF
{
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "url": "$FRONTEND_URL",
    "tests": {
        "homepage": { "passed": $([ "$RESPONSE" = "200" ] && echo "true" || echo "false") }
    },
    "summary": {
        "note": "Playwright not available - basic HTTP test only"
    }
}
EOF
fi

# Parse and display results
if [ -f "$LOG_DIR/e2e_results.json" ]; then
    echo -e "\n=== E2E Test Results ===" | tee -a "$LOG_DIR/e2e_summary.txt"
    
    # Extract test results
    jq -r '
        if .tests then
            "Homepage: " + (if .tests.homepage.passed then "PASS" else "FAIL" end),
            (if .tests.marketData then "Market Data: " + (if .tests.marketData.passed then "PASS" else "FAIL" end) else empty end),
            (if .tests.portfolio then "Portfolio: " + (if .tests.portfolio.passed then "PASS" else "FAIL" end) else empty end),
            (if .tests.orderFlow then "Order Flow: " + (if .tests.orderFlow.passed then "PASS" else "FAIL" end) else empty end)
        else
            "Results format error"
        end
    ' "$LOG_DIR/e2e_results.json" | tee -a "$LOG_DIR/e2e_summary.txt"
    
    # Performance metrics
    if [ -f "$LOG_DIR/e2e_results.json" ]; then
        LOAD_TIME=$(jq -r '.performance.pageLoadTime // 0' "$LOG_DIR/e2e_results.json")
        FPS=$(jq -r '.performance.fps // 0' "$LOG_DIR/e2e_results.json")
        
        echo -e "\nPerformance Metrics:" | tee -a "$LOG_DIR/e2e_summary.txt"
        echo "- Page Load Time: ${LOAD_TIME}ms" | tee -a "$LOG_DIR/e2e_summary.txt"
        echo "- FPS: $FPS" | tee -a "$LOG_DIR/e2e_summary.txt"
    fi
fi

echo -e "\nE2E test completed at: $(date)" | tee -a "$LOG_DIR/e2e_summary.txt"
echo "Results saved to: $LOG_DIR" | tee -a "$LOG_DIR/e2e_summary.txt"