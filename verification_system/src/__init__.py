"""
Continuum Git Hook Verification System

A modular, testable verification package that integrates with the RunArtifact system
for universal diagnostic capabilities across all Continuum run types.
"""

from .git_hook_verification import GitHookVerification
from .run_artifact_integration import RunArtifactIntegration
from .verification_history import VerificationHistory

__all__ = [
    'GitHookVerification',
    'RunArtifactIntegration', 
    'VerificationHistory'
]