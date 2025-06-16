#!/usr/bin/env python3
"""
Test App Store Style Validation Pipeline
Demonstrates multi-layer validation like Apple/Google app review process
"""

import asyncio
import sys
from pathlib import Path
import json
import time

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_app_store_validation():
    print("🏪 TESTING APP STORE STYLE VALIDATION PIPELINE")
    print("Multi-layer validation: Automated → AI → Human (like App Store)")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'app-store-validator',
            'agentName': 'App Store Validation Test',
            'agentType': 'ai'
        })
        
        # Test cases that should be rejected at different layers
        test_cases = [
            {
                'name': 'Obvious Malware - Layer 1 Rejection',
                'code': 'eval("alert(\'hacked\')"); os.system("rm -rf /")',
                'language': 'javascript',
                'expected_layer': 1,
                'expected_result': 'rejected'
            },
            {
                'name': 'Syntax Error - Layer 1 Rejection', 
                'code': 'var x = 10; if (x > 5 { console.log("missing paren"); }',
                'language': 'javascript',
                'expected_layer': 1,
                'expected_result': 'rejected'
            },
            {
                'name': 'Suspicious Pattern - Layer 2 AI Review',
                'code': 'document.body.innerHTML = userInput; setTimeout(() => {}, 1000);',
                'language': 'javascript',
                'expected_layer': 2,
                'expected_result': 'review_needed'
            },
            {
                'name': 'Clean Code - Full Approval',
                'code': 'var element = document.querySelector("#content"); element.textContent = "Hello";',
                'language': 'javascript',
                'expected_layer': 3,
                'expected_result': 'approved'
            },
            {
                'name': 'Python Command Injection - Layer 1 Rejection',
                'code': 'import os; user_cmd = input("cmd: "); os.system(user_cmd)',
                'language': 'python',
                'expected_layer': 1,
                'expected_result': 'rejected'
            },
            {
                'name': 'Valid Python - Full Approval',
                'code': 'def calculate(a, b): return a + b\nresult = calculate(10, 20)',
                'language': 'python',
                'expected_layer': 3,
                'expected_result': 'approved'
            }
        ]
        
        print(f"\n📋 Testing {len(test_cases)} code submissions through validation pipeline...\n")
        
        results = []
        total_time = 0
        
        for i, test_case in enumerate(test_cases, 1):
            print(f"--- TEST {i}: {test_case['name']} ---")
            print(f"Code ({test_case['language']}): {test_case['code'][:50]}...")
            
            start_time = time.time()
            
            # Submit code through validation pipeline
            result = await client.js.execute(f"""
                // Mock validation pipeline like App Store review
                const code = `{test_case['code']}`;
                const language = '{test_case['language']}';
                
                console.log('🛡️ Starting validation pipeline...');
                
                // LAYER 1: Automated Basic Validation (like App Store automated checks)
                console.log('🔍 Layer 1: Automated security/syntax checks...');
                
                // Check for immediate rejection patterns
                const dangerousPatterns = [
                    /eval\\s*\\(/g,
                    /os\\.system\\s*\\(/g,
                    /exec\\s*\\(/g,
                    /rm\\s+-rf/g
                ];
                
                for (const pattern of dangerousPatterns) {{
                    if (pattern.test(code)) {{
                        return {{
                            success: false,
                            layer: 1,
                            reason: 'Automated security check failed - dangerous pattern detected',
                            pattern: pattern.source,
                            processing_time_ms: Date.now() - Date.now(), // Instant
                            review_type: 'automated_rejection'
                        }};
                    }}
                }}
                
                // Basic syntax check for JavaScript
                if (language === 'javascript') {{
                    try {{
                        new Function(code);
                    }} catch (syntaxError) {{
                        return {{
                            success: false,
                            layer: 1,
                            reason: 'Automated syntax check failed',
                            error: syntaxError.message,
                            processing_time_ms: 1, // Instant
                            review_type: 'automated_rejection'
                        }};
                    }}
                }}
                
                console.log('✅ Layer 1: Passed automated checks');
                
                // LAYER 2: AI Analysis (like App Store AI review)
                console.log('🤖 Layer 2: AI pattern analysis...');
                
                const suspiciousPatterns = [
                    /innerHTML\\s*=/g,
                    /document\\.body/g,
                    /setTimeout.*1000/g
                ];
                
                let aiScore = 100;
                const aiIssues = [];
                
                suspiciousPatterns.forEach(pattern => {{
                    if (pattern.test(code)) {{
                        aiScore -= 20;
                        aiIssues.push(pattern.source);
                    }}
                }});
                
                if (aiScore < 70) {{
                    return {{
                        success: false,
                        layer: 2,
                        reason: 'AI review flagged security concerns',
                        ai_score: aiScore,
                        issues: aiIssues,
                        processing_time_ms: 50, // AI analysis takes longer
                        review_type: 'ai_rejection'
                    }};
                }}
                
                console.log(`✅ Layer 2: Passed AI review (score: ${{aiScore}})`);
                
                // LAYER 3: Final Approval (like App Store human review)
                console.log('👤 Layer 3: Final approval...');
                
                return {{
                    success: true,
                    layer: 3,
                    reason: 'Code approved through all validation layers',
                    ai_score: aiScore,
                    final_approval: true,
                    processing_time_ms: 100, // Full pipeline
                    review_type: 'full_approval'
                }};
            """)
            
            end_time = time.time()
            processing_time = (end_time - start_time) * 1000  # Convert to ms
            total_time += processing_time
            
            if result['success']:
                validation_data = json.loads(result['result'])
                
                print(f"📊 Result: {validation_data['review_type']}")
                print(f"🚦 Layer reached: {validation_data['layer']}")
                print(f"⏱️ Processing time: {processing_time:.1f}ms")
                
                if validation_data['success']:
                    print(f"✅ APPROVED: {validation_data['reason']}")
                    if 'ai_score' in validation_data:
                        print(f"🤖 AI Score: {validation_data['ai_score']}/100")
                else:
                    print(f"❌ REJECTED: {validation_data['reason']}")
                    if 'pattern' in validation_data:
                        print(f"🚨 Dangerous pattern: {validation_data['pattern']}")
                
                results.append({
                    'test_name': test_case['name'],
                    'layer_reached': validation_data['layer'],
                    'approved': validation_data['success'],
                    'processing_time_ms': processing_time,
                    'review_type': validation_data['review_type']
                })
            else:
                print(f"❌ Test execution failed")
                results.append({
                    'test_name': test_case['name'],
                    'layer_reached': 0,
                    'approved': False,
                    'processing_time_ms': processing_time,
                    'review_type': 'execution_failed'
                })
            
            print()
        
        # Summary statistics
        print("📊 VALIDATION PIPELINE STATISTICS")
        print("=" * 50)
        
        layer_1_rejects = len([r for r in results if r['layer_reached'] == 1 and not r['approved']])
        layer_2_rejects = len([r for r in results if r['layer_reached'] == 2 and not r['approved']])
        layer_3_approvals = len([r for r in results if r['layer_reached'] == 3 and r['approved']])
        
        print(f"📋 Total submissions: {len(results)}")
        print(f"🚫 Layer 1 rejections (automated): {layer_1_rejects} ({layer_1_rejects/len(results)*100:.1f}%)")
        print(f"🤖 Layer 2 rejections (AI): {layer_2_rejects} ({layer_2_rejects/len(results)*100:.1f}%)")
        print(f"✅ Layer 3 approvals (full): {layer_3_approvals} ({layer_3_approvals/len(results)*100:.1f}%)")
        print(f"⏱️ Average processing time: {total_time/len(results):.1f}ms")
        
        # Show efficiency gains
        automated_catches = layer_1_rejects
        total_rejections = layer_1_rejects + layer_2_rejects
        
        if total_rejections > 0:
            efficiency = (automated_catches / total_rejections) * 100
            print(f"⚡ Automation efficiency: {efficiency:.1f}% of bad code caught instantly")
        
        print(f"\n💡 Like App Store: Most garbage rejected instantly by automation,")
        print(f"   AI handles nuanced cases, human-level approval for clean code")
        
        return results

if __name__ == "__main__":
    results = asyncio.run(test_app_store_validation())
    print(f"\n🎉 App Store style validation pipeline test completed!")
    print(f"   {len([r for r in results if not r['approved']])} rejected, {len([r for r in results if r['approved']])} approved")