import React from "react";
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from "react-syntax-highlighter/dist/esm/styles/prism";
import './MessageContent.css';

function MessageContent({ content, format = 'TEXT', roomMembers = [] }) {
    const renderMentionsInText = (text) => {
        if (!text) return text;

        const mentionRegex = /@(\w+|everyone)/g;
        const parts = [];
        let lastIndex = 0;
        let match;

        while ((match = mentionRegex.exec(text)) !== null) {
            if (match.index > lastIndex) {
                parts.push(text.substring(lastIndex, match.index));
            }

            parts.push(
                <span
                    key={match.index}
                    className={`mention ${match[1] === 'everyone' ? 'mention-everyone' : ''}`}
                > @{match[1]}</span>
            );
            lastIndex = match.index + match[0].length;
        }

        if (lastIndex < text.length) {
            parts.push(text.substring(lastIndex));
        }

        return parts.length > 0 ? parts : text;
    };

    const renderTextWithMentions = ({ children }) => {
        if(typeof children === 'string') {
            return <>{renderMentionsInText(children)}</>;
        }
        return <>{children}</>;
    };
    
    const isInCodeBlock = (node) => {
        if (!node || !node.parentElement) return false;

        let parent = node.parentElement;
        while (parent) {
            if (parent.tagName === 'CODE' || parent.tagName === 'PRE') {
                return true;
            }
            parent = parent.parentElement;
        }
        return false;
    };

    if (format === 'MARKDOWN') {
        return (
            <div className="message-content-markdown">
                <ReactMarkdown
                    remarkPlugins={[remarkGfm, remarkBreaks]}
                    components={{
                        code({ node, inline, className, children, ...props }) {
                            const match = /language-(\w+)/.exec(className || '');
                            return !inline && match ? (
                                <SyntaxHighlighter
                                    style={vscDarkPlus}
                                    language={match[1]}
                                    PreTag="div"
                                    {...props}
                                >
                                    {String(children).replace(/\n$/, '')}
                                </SyntaxHighlighter>
                            ) : (
                                <code className={className} {...props}>
                                    {children}
                                </code>
                            );
                        },
                        text: renderTextWithMentions,
                        a({ node, children, href, ...props }) {
                            return (
                                <a
                                    href={href}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    {...props}
                                >{children}</a>
                            );
                        }
                    }}
                    >
                        {content}
                    </ReactMarkdown>
            </div>
        );
    }

    return (
        <div className="message-content-text">
            {renderMentionsInText(content)}
        </div>
    );
}

export default MessageContent;